use inflector::Inflector;
use proc_macro2::{Span, TokenStream as TokenStream2};
use proc_macro_error::OptionExt;
use quote::quote;
use std::convert::*;

use super::*;

pub fn derive_deserialize(ident: &syn::Ident, st: &syn::DataStruct) -> TokenStream2 {
    let name = ident.to_string().to_screaming_snake_case();
    let def_visitor_tt = def_visitor(ident, &name, st);
    let impl_deserialize_tt = impl_deserialize(ident, &name, st);
    quote! {
        #def_visitor_tt
        #impl_deserialize_tt
    } // quote!
}

pub fn derive_holder(ident: &syn::Ident, st: &syn::DataStruct, attr: &HolderAttr) -> TokenStream2 {
    let name = ident.to_string().to_screaming_snake_case();
    let holder_ident = as_holder_ident(ident);
    let def_holder_tt = def_holder(ident, st);
    let impl_holder_tt = impl_holder(ident, attr, st);
    let impl_entity_table_tt = impl_entity_table(ident, attr);
    if attr.generate_deserialize {
        let def_visitor_tt = def_visitor(&holder_ident, &name, st);
        let impl_deserialize_tt = impl_deserialize(&holder_ident, &name, st);
        let impl_with_visitor_tt = impl_with_visitor(ident);
        quote! {
            #def_holder_tt
            #impl_holder_tt
            #impl_entity_table_tt
            #def_visitor_tt
            #impl_deserialize_tt
            #impl_with_visitor_tt
        }
    } else {
        quote! {
            #def_holder_tt
            #impl_holder_tt
            #impl_entity_table_tt
        }
    }
}

/// This must be same between codegens
fn table_arg() -> syn::Ident {
    syn::Ident::new("table", Span::call_site())
}

struct FieldEntries {
    attributes: Vec<syn::Ident>,
    holder_types: Vec<syn::Type>,
    into_owned: Vec<TokenStream2>,
}

impl FieldEntries {
    fn parse(st: &syn::DataStruct) -> Self {
        let table_arg = table_arg();

        let mut attributes = Vec::new();
        let mut holder_types = Vec::new();
        let mut into_owned = Vec::new();

        for field in &st.fields {
            let ident = field.ident.as_ref().expect_or_abort("st is not struct");
            attributes.push(ident.clone());

            let ft: FieldType = field.ty.clone().try_into().unwrap();

            let HolderAttr { place_holder, .. } = HolderAttr::parse(&field.attrs);
            if place_holder {
                match &ft {
                    FieldType::Path(_) => {
                        into_owned.push(quote! { #ident.into_owned(#table_arg)? });
                    }
                    FieldType::Optional(_) => {
                        into_owned.push(quote! { #ident.map(|holder| holder.into_owned(#table_arg)).transpose()? });
                    }
                    FieldType::List(_) => into_owned.push(quote! {
                        #ident
                            .into_iter()
                            .map(|v| v.into_owned(#table_arg))
                            .collect::<::std::result::Result<Vec<_>, _>>()?
                    }),
                    FieldType::Boxed(_) => abort_call_site!("Unexpected Box<T>"),
                }
                holder_types.push(ft.as_holder().as_place_holder().into());
            } else {
                into_owned.push(quote! { #ident });
                holder_types.push(ft.into());
            }
        }
        FieldEntries {
            attributes,
            holder_types,
            into_owned,
        }
    }
}

pub fn def_holder(ident: &syn::Ident, st: &syn::DataStruct) -> TokenStream2 {
    let holder_ident = as_holder_ident(ident);
    let FieldEntries {
        attributes,
        holder_types,
        ..
    } = FieldEntries::parse(st);
    quote! {
        /// Auto-generated by `#[derive(Holder)]`
        #[derive(Debug, Clone, PartialEq)]
        pub struct #holder_ident {
            #( pub #attributes: #holder_types ),*
        }
    }
}

pub fn impl_holder(ident: &syn::Ident, table: &HolderAttr, st: &syn::DataStruct) -> TokenStream2 {
    let name = ident.to_string().to_screaming_snake_case();
    let holder_ident = as_holder_ident(ident);
    let FieldEntries {
        attributes,
        into_owned,
        ..
    } = FieldEntries::parse(st);
    let attr_len = attributes.len();
    let HolderAttr { table, .. } = table;
    let table_arg = table_arg();
    let ruststep = ruststep_crate();

    quote! {
        #[automatically_derived]
        impl #ruststep::tables::Holder for #holder_ident {
            type Table = #table;
            type Owned = #ident;
            fn into_owned(self, #table_arg: &Self::Table) -> #ruststep::error::Result<Self::Owned> {
                let #holder_ident { #(#attributes),* } = self;
                Ok(#ident { #(#attributes: #into_owned),* })
            }
            fn name() -> &'static str {
                #name
            }
            fn attr_len() -> usize {
                #attr_len
            }
        }
    } // quote!
}

pub fn impl_entity_table(ident: &syn::Ident, table: &HolderAttr) -> TokenStream2 {
    let HolderAttr { table, field, .. } = table;
    let holder_ident = as_holder_ident(ident);
    let ruststep = ruststep_crate();

    quote! {
        #[automatically_derived]
        impl #ruststep::tables::EntityTable<#holder_ident> for #table {
            fn get_owned(&self, entity_id: u64) -> #ruststep::error::Result<#ident> {
                #ruststep::tables::get_owned(self, &self.#field, entity_id)
            }
            fn owned_iter<'table>(&'table self) -> Box<dyn Iterator<Item = #ruststep::error::Result<#ident>> + 'table> {
                #ruststep::tables::owned_iter(self, &self.#field)
            }
        }
    }
}

// `name` may be different from `ident`
// because this will be used for both Entity struct and its `*Holder` struct.
fn def_visitor(ident: &syn::Ident, name: &str, st: &syn::DataStruct) -> TokenStream2 {
    let visitor_ident = as_visitor_ident(ident);
    let FieldEntries { attributes, .. } = FieldEntries::parse(st);
    let attr_len = attributes.len();
    quote! {
        #[doc(hidden)]
        pub struct #visitor_ident;

        #[automatically_derived]
        impl<'de> ::serde::de::Visitor<'de> for #visitor_ident {
            type Value = #ident;
            fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                write!(formatter, #name)
            }

            fn visit_seq<A>(self, mut seq: A) -> ::std::result::Result<Self::Value, A::Error>
            where
                A: ::serde::de::SeqAccess<'de>,
            {
                if let Some(size) = seq.size_hint() {
                    if size != #attr_len {
                        use ::serde::de::Error;
                        return Err(A::Error::invalid_length(size, &self));
                    }
                }
                #( let #attributes = seq.next_element()?.unwrap(); )*
                Ok(#ident { #(#attributes),* })
            }

            // Entry point for Record or Parameter::Typed
            fn visit_map<A>(self, mut map: A) -> ::std::result::Result<Self::Value, A::Error>
            where
                A: ::serde::de::MapAccess<'de>,
            {
                let key: String = map
                    .next_key()?
                    .expect("Empty map cannot be accepted as ruststep Holder"); // this must be a bug, not runtime error
                if key != #name {
                    use ::serde::de::{Error, Unexpected};
                    return Err(A::Error::invalid_value(Unexpected::Other(&key), &self));
                }
                let value = map.next_value()?; // send to Self::visit_seq
                Ok(value)
            }
        }
    } // quote!
}

// `name` may be different from `ident`
// because this will be used for both Entity struct and its `*Holder` struct.
fn impl_deserialize(ident: &syn::Ident, name: &str, st: &syn::DataStruct) -> TokenStream2 {
    let visitor_ident = as_visitor_ident(ident);
    let FieldEntries { attributes, .. } = FieldEntries::parse(st);
    let attr_len = attributes.len();
    quote! {
        #[automatically_derived]
        impl<'de> ::serde::de::Deserialize<'de> for #ident {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
            where
                D: ::serde::de::Deserializer<'de>,
            {
                deserializer.deserialize_tuple_struct(#name, #attr_len, #visitor_ident {})
            }
        }
    } // quote!
}

fn impl_with_visitor(ident: &syn::Ident) -> TokenStream2 {
    let ruststep = ruststep_crate();

    let visitor_ident = as_holder_visitor(ident);
    let holder_ident = as_holder_ident(ident);

    quote! {
        #[automatically_derived]
        impl #ruststep::tables::WithVisitor for #holder_ident {
            type Visitor = #visitor_ident;
            fn visitor_new() -> Self::Visitor {
                #visitor_ident {}
            }
        }
    } // quote!
}
