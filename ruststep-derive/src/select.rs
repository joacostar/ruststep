use super::*;
use inflector::Inflector;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro_error::*;
use quote::quote;

struct Input {
    name: String,
    table: syn::Path,
    ident: syn::Ident,
    holder_ident: syn::Ident,
    holder_visitor_ident: syn::Ident,
    variants: Vec<syn::Ident>,
    variant_names: Vec<String>,
    variant_exprs: Vec<TokenStream2>,
    holder_types: Vec<syn::Type>,
    holder_exprs: Vec<TokenStream2>,
    table_fields: Vec<Option<syn::Ident>>,
}

impl Input {
    fn parse(ident: &syn::Ident, e: &syn::DataEnum, attr: &HolderAttr) -> Self {
        let name = ident.to_string().to_screaming_snake_case();
        let holder_ident = as_holder_ident(ident);
        let holder_visitor_ident = as_visitor_ident(&holder_ident);
        let variants: Vec<syn::Ident> = e.variants.iter().map(|var| var.ident.clone()).collect();
        let variant_names: Vec<_> = variants
            .iter()
            .map(|id| id.to_string().to_screaming_snake_case())
            .collect();
        let table = attr
            .table
            .clone()
            .expect_or_abort("table attribute is lacked");

        let mut holder_exprs = Vec::new();
        let mut holder_types = Vec::new();
        let mut table_fields = Vec::new();
        let mut variant_exprs = Vec::new();
        for var in &e.variants {
            let HolderAttr {
                field,
                place_holder,
                ..
            } = HolderAttr::parse(&var.attrs);
            table_fields.push(field);

            assert_eq!(var.fields.len(), 1);
            for f in &var.fields {
                let ty = FieldType::try_from(f.ty.clone()).unwrap();
                if let FieldType::Boxed(_) = ty {
                    if place_holder {
                        // ENTITY case
                        holder_types.push(as_holder_path(&f.ty));
                        holder_exprs.push(quote! { Box::new(sub.into_owned(table)?) });
                        variant_exprs.push(quote! { Box::new(owned) });
                    } else {
                        abort_call_site!("Simple type should not be Boxed")
                    }
                } else {
                    variant_exprs.push(quote! { owned });
                    if place_holder {
                        // *Any case
                        holder_types.push(as_holder_path(&f.ty));
                        holder_exprs.push(quote! { sub.into_owned(table)? });
                    } else {
                        // SimpleType case
                        holder_types.push(f.ty.clone());
                        holder_exprs.push(quote! { sub });
                    }
                }
            }
        }

        Input {
            name,
            table,
            ident: ident.clone(),
            holder_ident,
            holder_visitor_ident,
            variants,
            variant_names,
            variant_exprs,
            holder_types,
            holder_exprs,
            table_fields,
        }
    }

    fn def_holder(&self) -> TokenStream2 {
        let Input {
            holder_ident,
            variants,
            holder_types,
            ..
        } = self;
        quote! {
            #[derive(Clone, Debug, PartialEq)]
            enum #holder_ident {
                #(#variants(#holder_types)),*
            }
        } // quote!
    }

    fn impl_holder(&self) -> TokenStream2 {
        let Input {
            name,
            ident,
            holder_ident,
            variants,
            table,
            holder_exprs,
            ..
        } = self;
        let ruststep = ruststep_crate();

        quote! {
            impl #ruststep::tables::Holder for #holder_ident {
                type Owned = #ident;
                type Table = #table;
                fn into_owned(self, table: &Self::Table) -> #ruststep::error::Result<Self::Owned> {
                    Ok(match self {
                        #(#holder_ident::#variants(sub) => #ident::#variants(#holder_exprs)),*
                    })
                }
                fn name() -> &'static str {
                    #name
                }
                fn attr_len() -> usize {
                    0
                }
            }
        } // quote!
    }

    fn impl_deserialize(&self) -> TokenStream2 {
        let Input {
            name,
            holder_ident,
            holder_visitor_ident,
            ..
        } = self;
        quote! {
            impl<'de> ::serde::de::Deserialize<'de> for #holder_ident {
                fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
                where
                    D: ::serde::de::Deserializer<'de>,
                {
                    deserializer.deserialize_tuple_struct(#name, 0, #holder_visitor_ident {})
                }
            }
        } // quote!
    }

    fn def_visitor(&self) -> TokenStream2 {
        let Input {
            holder_ident,
            holder_visitor_ident,
            name,
            variants,
            variant_names,
            variant_exprs,
            ..
        } = self;
        let ruststep = ruststep_crate();

        quote! {
            struct #holder_visitor_ident;

            impl<'de> ::serde::de::Visitor<'de> for #holder_visitor_ident {
                type Value = #holder_ident;
                fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                    write!(formatter, #name)
                }

                // Entry point for Record or Parameter::Typed
                fn visit_map<A>(self, mut map: A) -> ::std::result::Result<Self::Value, A::Error>
                where
                    A: ::serde::de::MapAccess<'de>,
                {
                    let key: String = map
                        .next_key()?
                        .expect("Empty map cannot be accepted as ruststep Holder"); // this must be a bug, not runtime error
                    match key.as_str() {
                        #(
                        #variant_names => {
                            let owned = map.next_value()?;
                            return Ok(#holder_ident::#variants(#variant_exprs));
                        }
                        )*
                        _ => {
                            use ::serde::de::{Error, Unexpected};
                            return Err(A::Error::invalid_value(Unexpected::Other(&key), &self));
                        }
                    }
                }
            }

            impl #ruststep::tables::WithVisitor for #holder_ident {
                type Visitor = #holder_visitor_ident;
                fn visitor_new() -> Self::Visitor {
                    #holder_visitor_ident {}
                }
            }
        } // quote!
    }

    fn impl_entity_table(&self) -> TokenStream2 {
        let Input {
            ident,
            holder_ident,
            variants,
            table_fields,
            table,
            variant_exprs,
            ..
        } = self;
        let ruststep = ruststep_crate();
        let mut vars = Vec::new();
        let mut fields = Vec::new();
        let mut exprs = Vec::new();
        for ((var, field), expr) in variants
            .iter()
            .zip(table_fields.iter())
            .zip(variant_exprs.iter())
        {
            if let Some(field) = field {
                vars.push(var);
                fields.push(field);
                exprs.push(expr);
            }
        }

        quote! {
            impl #ruststep::tables::EntityTable<#holder_ident> for #table {
                fn get_owned(&self, entity_id: u64) -> #ruststep::error::Result<#ident> {
                    #(
                    if let Ok(owned) = #ruststep::tables::get_owned(self, &self.#fields, entity_id) {
                        return Ok(#ident::#vars(#exprs));
                    }
                    )*
                    Err(#ruststep::error::Error::UnknownEntity(entity_id))
                }
                fn owned_iter<'table>(&'table self) -> Box<dyn Iterator<Item = #ruststep::error::Result<#ident>> + 'table> {
                    Box::new(::itertools::chain![
                        #(
                        #ruststep::tables::owned_iter(self, &self.#fields)
                            .map(|owned| owned.map(|owned| #ident::#vars(#exprs)))
                        ),*
                    ])
                }
            }
        } // quote!
    }
}

pub fn derive_holder(ident: &syn::Ident, e: &syn::DataEnum, attr: &HolderAttr) -> TokenStream2 {
    let input = Input::parse(ident, e, attr);
    let def_holder_tt = input.def_holder();
    let impl_holder_tt = input.impl_holder();

    if attr.generate_deserialize {
        let impl_deserialize_tt = input.impl_deserialize();
        let def_visitor_tt = input.def_visitor();
        let impl_entity_table_tt = input.impl_entity_table();
        quote! {
            #def_holder_tt
            #impl_holder_tt
            #impl_deserialize_tt
            #def_visitor_tt
            #impl_entity_table_tt
        } // quote!
    } else {
        quote! {
            #def_holder_tt
            #impl_holder_tt
        } // quote!
    }
}

pub fn derive_deserialize(_ident: &syn::Ident, _e: &syn::DataEnum) -> TokenStream2 {
    unimplemented!()
}