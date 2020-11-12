use super::*;
use derive_more::From;
use nom::{branch::*, bytes::complete::*, combinator::*, IResult, Parser};

pub struct Expr {}

/// ```text
/// 216 expression = simple_expression [ rel_op_extended simple_expression ] .
/// 305 simple_expression = term { add_like_op term } .
/// 325 term = factor { multiplication_like_op factor } .
/// 217 factor = simple_factor [ `**` simple_factor ] .
/// ```
pub fn expr(_input: &str) -> IResult<&str, Expr> {
    todo!()
}

#[derive(Debug, Clone, PartialEq, From)]
pub enum SimpleFactor {
    #[from(ignore)]
    Primary {
        unary_op: Option<UnaryOp>,
        primary: Primary,
    },
}

impl From<Primary> for SimpleFactor {
    fn from(primary: Primary) -> Self {
        SimpleFactor::Primary {
            unary_op: None,
            primary,
        }
    }
}

impl From<(UnaryOp, Primary)> for SimpleFactor {
    fn from((unary_op, primary): (UnaryOp, Primary)) -> Self {
        SimpleFactor::Primary {
            unary_op: Some(unary_op),
            primary,
        }
    }
}

/// 306 simple_factor = aggregate_initializer
///                   | entity_constructor
///                   | enumeration_reference
///                   | interval
///                   | query_expression
///                   | ( [ unary_op ] ( `(` expression `)` | primary ) ) .
pub fn simple_factor(input: &str) -> IResult<&str, SimpleFactor> {
    // FIXME most branches are not supported
    tuple((
        opt(tuple((unary_op, multispace0)).map(|(op, _)| op)),
        primary,
    ))
    .map(|(unary_op, primary)| SimpleFactor::Primary { unary_op, primary })
    .parse(input)
}

#[derive(Debug, Clone, PartialEq, From)]
pub enum Primary {
    Literal(Literal),
}

/// 269 primary = literal | ( qualifiable_factor { qualifier } ) .
///
/// Example
/// --------
///
/// ```
/// use exp2rs::parser::*;
/// use nom::Finish;
///
/// let (residual, p) = primary("123").finish().unwrap();
/// assert_eq!(p, Literal::Real(123.0).into());
/// assert_eq!(residual, "");
/// ```
pub fn primary(input: &str) -> IResult<&str, Primary> {
    // FIXME add qualifiable_factor branch
    literal
        .map(|literal| Primary::Literal(literal))
        .parse(input)
}

#[derive(Debug, Clone, PartialEq, From)]
pub enum RelOpExtended {
    RelOp(RelOp),
    /// `IN`
    In,
    /// `LIKE`
    Like,
}

/// 283 rel_op_extended = rel_op | `IN` | `LIKE` .
pub fn rel_op_extended(input: &str) -> IResult<&str, RelOpExtended> {
    use RelOpExtended::*;
    alt((
        rel_op.map(|op| RelOp(op)),
        alt((value(In, tag("IN")), value(Like, tag("LIKE")))),
    ))
    .parse(input)
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelOp {
    /// `=`
    Equal,
    /// `<>`
    NotEqual,
    /// `<`
    LT,
    /// `>`
    GT,
    /// `<=`
    LEQ,
    /// `>=`
    GEQ,
    /// `:=:`
    InstanceEqual,
    /// `:<>:`
    InstanceNotEqual,
}

/// 282 rel_op = `<` | `>` | `<=` | `>=` | `<>` | `=` | `:<>:` | `:=:` .
pub fn rel_op(input: &str) -> IResult<&str, RelOp> {
    use RelOp::*;
    alt((
        value(Equal, tag("=")),
        value(NotEqual, tag("<>")),
        value(LT, tag("<")),
        value(GT, tag(">")),
        value(LEQ, tag("<=")),
        value(GEQ, tag(">=")),
        value(InstanceEqual, tag(":=:")),
        value(InstanceNotEqual, tag(":<>:")),
    ))
    .parse(input)
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `NOT`
    Not,
}

/// 331 unary_op = `+` | `-` | `NOT` .
pub fn unary_op(input: &str) -> IResult<&str, UnaryOp> {
    use UnaryOp::*;
    alt((
        value(Plus, tag("+")),
        value(Minus, tag("-")),
        value(Not, tag("NOT")),
    ))
    .parse(input)
}

#[derive(Debug, Clone, PartialEq)]
pub enum MultiplicationLikeOp {
    /// `*`
    Mul,
    /// `/`
    RealDiv,
    /// `DIV`
    IntegerDiv,
    /// `MOD`
    Mod,
    /// `AND`
    And,
    /// `||`, Complex entity instance construction operator (12.10)
    ComplexEntityInstanceConstruction,
}

/// 257 multiplication_like_op = `*` | `/` | `DIV` | `MOD` | `AND` | `||` .
pub fn multiplication_like_op(input: &str) -> IResult<&str, MultiplicationLikeOp> {
    use MultiplicationLikeOp::*;
    alt((
        value(Mul, tag("*")),
        value(RealDiv, tag("/")),
        value(IntegerDiv, tag("DIV")),
        value(Mod, tag("MOD")),
        value(And, tag("AND")),
        value(ComplexEntityInstanceConstruction, tag("||")),
    ))
    .parse(input)
}

#[derive(Debug, Clone, PartialEq)]
pub enum AddLikeOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `OR`
    Or,
    /// `XOR`
    Xor,
}

/// 168 add_like_op = `+` | `-` | `OR` | `XOR` .
pub fn add_like_op(input: &str) -> IResult<&str, AddLikeOp> {
    use AddLikeOp::*;
    alt((
        value(Add, tag("+")),
        value(Sub, tag("-")),
        value(Or, tag("OR")),
        value(Xor, tag("XOR")),
    ))
    .parse(input)
}
