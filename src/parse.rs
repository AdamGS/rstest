use syn::{Expr, Ident, Lit, LitStr, Meta, NestedMeta, parse::{Parse, ParseStream, Result, Error}, spanned::Spanned, punctuated::{Punctuated}, Token, token, MetaList};
use proc_macro2::{TokenStream, Span};
use crate::error::error;
use cfg_if::cfg_if;
// TODO: Remove this dependency
use quote::quote;
use quote::ToTokens;

#[derive(Default, Debug)]
pub struct ParametrizeData {
    pub args: Vec<Ident>,
    pub cases: Vec<TestCase>,
}

#[derive(Default, Debug)]
/// Parametrize
pub struct ParametrizeInfo {
    pub data: ParametrizeData,
    pub modifiers: Modifiers,
}

#[derive(Debug)]
/// A test case instance data. Contains a list of arguments. It is parsed by parametrize
/// attributes.
pub struct TestCase {
    pub args: Vec<CaseArg>,
    pub description: Option<Ident>
}

impl Parse for TestCase {
    fn parse(input: ParseStream) -> Result<Self> {
        let case: Ident = input.parse()?;
        if case == "case" {
            let mut description = None;
            if input.peek(Token![::]) {
                let _ = input.parse::<Token![::]>();
                description = Some(input.parse()?);
            }
            let content;
            let _ = syn::parenthesized!(content in input);
            let args = Punctuated::<CaseArg, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
            Ok(TestCase { args, description })
        } else {
            Err(Error::new(case.span(), "expected a test case"))
        }
    }
}

impl TestCase {
    pub fn span_start(&self) -> Span {
        self.args.first().map(|arg| arg.span()).unwrap_or(Span::call_site())
    }
    pub fn span_end(&self) -> Span {
        self.args.last().map(|arg| arg.span()).unwrap_or(Span::call_site())
    }
}

#[derive(Debug, Clone)]
/// A test case's argument as an expression that can be assigned.
pub struct CaseArg {
    expr: Expr,
}

impl CaseArg {
    pub fn new(expr: Expr) -> Self {
        Self { expr }
    }
}

#[cfg(test)]
impl PartialEq for CaseArg {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

impl ToTokens for CaseArg {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.expr.to_tokens(tokens);
    }
}

impl From<Expr> for CaseArg {
    fn from(expr: Expr) -> Self {
        CaseArg::new(expr)
    }
}


struct UnwrapRustCode(Expr);

impl Parse for UnwrapRustCode {
    fn parse(input: ParseStream) -> Result<Self> {
        let nested: NestedMeta = input.parse()?;
        Self::report_deprecated(&nested);
        let arg = Self::get_unwrap(&nested)?;
        let tokens = arg.nested.first()
                .map(|m| *m.value())
                .and_then(Self::nested_meta_literal_str)
                .map(Self::compile_lit_str)
                .unwrap_or_else(
                        || Ok(error(&format!("Invalid {} argument",
                                             arg.ident),
                                        nested.span(), nested.span()))
                )?;
        syn::parse2(tokens).map(UnwrapRustCode)
    }
}

impl UnwrapRustCode {
    const UNWRAP_NAME: &'static str = "Unwrap";

    fn peek(input: ParseStream) -> bool {
        input.fork().parse::<NestedMeta>().map( |nested|
            Self::get_unwrap(&nested).is_ok()
        ).unwrap_or(false)
    }

    fn get_unwrap(nested: &NestedMeta) -> Result<&MetaList> {
        match nested {
            NestedMeta::Meta(Meta::List(ref arg)) if arg.ident == Self::UNWRAP_NAME => Ok(arg),
            _ => return Err(Error::new(nested.span(), "Not a valid string rust code"))
        }
    }

    fn report_deprecated(nested: &NestedMeta) {
        cfg_if! {
        if #[cfg(use_proc_macro_diagnostic)] {
            fn inner(nested: &NestedMeta) {
                nested.span()
                    .unwrap()
                    .warning("Deprecated: Case argument accepts arbitrary rust code now.")
                    .emit();
            }
        } else {
            fn inner(nested: &NestedMeta) {
                match nested {
                    NestedMeta::Meta(Meta::List(arg)) => {
                        arg.nested.first()
                            .map(|m| *m.value())
                            .and_then(UnwrapRustCode::nested_meta_literal_str)
                            .map(|content| {
                                eprintln!(r#"{}("<code>") is deprecated. Case argument accepts arbitrary rust code now."#,
                                          arg.ident);
                                content
                            })
                            .unwrap();
                    }
                    _ => { unreachable!() }
                }
            }
        }
        }
        inner(nested);
    }

    fn nested_meta_literal_str(nested: &NestedMeta) -> Option<&LitStr> {
        match nested {
            NestedMeta::Literal(Lit::Str(lit)) => Some(lit),
            _ => None
        }
    }

    fn compile_lit_str(lit: &LitStr) -> Result<TokenStream> {
        lit.parse::<Expr>()
            .map(|e| quote! { #e })
            .or_else(|e| Err(Error::new(
                lit.span(),
                &format!("Cannot parse '{}' due {}", lit.value(), e),
            ))
            )
    }
}

impl Parse for CaseArg {
    fn parse(input: ParseStream) -> Result<Self> {
        if UnwrapRustCode::peek(input) {
            Ok(CaseArg::new(input.parse::<UnwrapRustCode>()?.0))
        } else {
            input.parse()
                .map(CaseArg::new)
                .map_err(|e| Error::new(
                    e.span(),
                    format!("Cannot parse due {}", e)
                )
            )
        }
    }
}

#[derive(Default, Debug, PartialEq)]
pub struct Modifiers {
    pub modifiers: Vec<RsTestAttribute>
}

impl Parse for Modifiers {
    fn parse(input: ParseStream) -> Result<Self> {
        let vars = Punctuated::<RsTestAttribute, Token![::]>::parse_terminated(input)?;
        Ok(Modifiers {
            modifiers: vars.into_iter().collect(),
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum RsTestAttribute {
    Attr(Ident),
    Tagged(Ident, Vec<Ident>),
    Type(Ident, syn::Type)
}

fn no_literal_nested(nested: NestedMeta) -> Result<Meta> {
    match nested {
        NestedMeta::Meta(m) => Ok(m),
        NestedMeta::Literal(l) => Err(Error::new(l.span(), "Unexpected literal"))
    }
}

fn just_word_meta(meta: Meta) -> Result<Ident> {
    match meta {
        Meta::Word(ident) => Ok(ident),
        other => Err(Error::new(other.span(), "Should be an ident"))
    }
}

impl Parse for RsTestAttribute {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek2(Token![<]) {
            let tag = input.parse()?;
            let _open = input.parse::<Token![<]>()?;
            let inner = input.parse()?;
            let _close = input.parse::<Token![>]>()?;
            Ok(RsTestAttribute::Type(tag, inner))
        } else {
            let meta = no_literal_nested(NestedMeta::parse(input)?)?;
            use Meta::*;
            match meta {
                Word(ident) => Ok(RsTestAttribute::Attr(ident)),
                List(l) =>
                    Ok(RsTestAttribute::Tagged(l.ident,
                                               l.nested.into_iter()
                                                   .map(no_literal_nested)
                                                   .collect::<Result<Vec<Meta>>>()?
                                                   .into_iter().map(just_word_meta)
                                                   .collect::<Result<Vec<Ident>>>()?,
                    )),
                NameValue(nv) => Err(Error::new(nv.span(), "Invalid attribute"))
            }
        }
    }
}

impl Parse for ParametrizeData {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut args = vec![];
        loop {
            if input.peek2(token::Paren) || input.peek2(Token![::]) {
                break;
            }
            if let Ok(arg) = input.parse() {
                args.push(arg);
                if input.parse::<Token![,]>().is_err() {
                    break;
                }
            } else {
                break;
            }
        }

        let cases: Vec<_> =
            Punctuated::<Option<TestCase>, Token![,]>::parse_separated_nonempty_with(
                input, |input_tokens|
                    if input_tokens.is_empty() { Ok(None) } else {
                        TestCase::parse(input_tokens).map(|inner| Some(inner))
                    }
            )?.into_iter().filter_map(|it| it).collect();
        Ok(ParametrizeData {
            args,
            cases,
        })
    }
}

impl Parse for ParametrizeInfo {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse a `ParametrizeData` and then look for `Modifiers` after `::` token if any.
        // Example
        //  |-    u,a,d
        //  |-    case(42, r("A{}"), Unwrap("D{}"))
        //  |  ::trace::notrace(a))
        //  |    ^^^^^^^^^^^^^^^^^^ Modifiers
        //  |______________________ ParametrizeData
        Ok(
            ParametrizeInfo {
                data: input.parse()?,
                modifiers: input.parse::<Token![::]>()
                    .or_else(|_| Ok(Default::default()))
                    .and_then(|_| input.parse())?,
            }
        )
    }
}

#[cfg(test)]
mod should {
    #[allow(unused_imports)]
    use pretty_assertions::assert_eq;

    use syn::{parse_str, ItemFn, parse2};
    use quote::quote;
    use super::*;
    use proc_macro2::{TokenTree};

    fn parse_meta<T: syn::parse::Parse, S: AsRef<str>>(test_case: S) -> T {
        let to_parse = format!(r#"
        #[{}]
        fn to_parse() {{}}
        "#, test_case.as_ref());

        let item_fn = parse_str::<ItemFn>(&to_parse).unwrap();

        let tokens = quote!(
            #item_fn
        );

        let tt = tokens.into_iter().skip(1).next().unwrap();

        if let TokenTree::Group(g) = tt {
            let ts = g.stream();
            parse2::<T>(ts).unwrap()
        } else {
            panic!("Cannot find group in {:#?}", tt)
        }
    }

    impl From<String> for CaseArg {
        fn from(data: String) -> Self {
            CaseArg::new(parse_str::<syn::Expr>(data.as_ref()).unwrap())
        }
    }

    impl<'a> From<&'a str> for CaseArg {
        fn from(data: &str) -> Self {
            CaseArg::new(parse_str::<syn::Expr>(data).unwrap())
        }
    }

    #[derive(Debug, PartialEq)]
    struct Args(Vec<CaseArg>);

    impl TestCase {
        fn args(&self) -> Args {
            Args(self.args.iter().cloned().collect())
        }
    }

    impl<'a, IT: Iterator<Item=&'a ToString>> From<IT> for Args {
        fn from(refs: IT) -> Self {
            Args(refs.map(|s| CaseArg::from(s.to_string())).collect())
        }
    }

    macro_rules! to_args {
        ($e:expr) => {Args::from($e.iter().map(|s| s as &ToString))};
    }

    macro_rules! to_strs {
        ($e:expr) => {$e.iter().map(ToString::to_string).collect::<Vec<_>>()};
    }

    mod parse_modifiers {
        use super::assert_eq;
        use super::*;

        fn parse_modifiers<S: AsRef<str>>(modifiers: S) -> Modifiers {
            parse_meta(modifiers)
        }

        trait IntoIdent {
            fn into_ident(self) -> Ident;
        }

        impl<S: AsRef<str>> IntoIdent for S {
            fn into_ident(self) -> Ident {
                parse_str(self.as_ref()).unwrap()
            }
        }

        impl RsTestAttribute {
            fn attr<I: IntoIdent>(i: I) -> Self {
                RsTestAttribute::Attr(i.into_ident())
            }

            fn tagged<I: IntoIdent, A: IntoIdent>(tag: I, attrs: Vec<A>) -> Self {
                RsTestAttribute::Tagged(
                    tag.into_ident(),
                    attrs.into_iter()
                        .map(|a| a.into_ident())
                        .collect()
                )
            }

            fn typed<I: IntoIdent, T: AsRef<str>>(tag: I, inner: T) -> Self {
                RsTestAttribute::Type(
                    tag.into_ident(),
                    parse_str(inner.as_ref()).unwrap()
                )
            }
        }

        #[test]
        fn one_simple_ident() {
            let modifiers = parse_modifiers("my_ident");

            let expected = Modifiers{ modifiers: vec![
                RsTestAttribute::attr("my_ident")
            ]
            };

            assert_eq!(expected, modifiers);
        }

        #[test]
        fn one_simple_group() {
            let modifiers = parse_modifiers("group_tag(first, second)");

            let expected = Modifiers{ modifiers: vec![
                RsTestAttribute::tagged("group_tag", vec!["first", "second"])
            ] };

            assert_eq!(expected, modifiers);
        }

        #[test]
        fn one_simple_type() {
            let modifiers = parse_modifiers("type_tag<(u32, T, (String, i32))>");

            let expected = Modifiers{ modifiers: vec![
                RsTestAttribute::typed("type_tag", "(u32, T, (String, i32))")
            ] };

            assert_eq!(expected, modifiers);
        }

        #[test]
        fn integrated() {
            let modifiers = parse_modifiers(r#"
            simple :: tagged(first, second) :: type_tag<(u32, T, (std::string::String, i32))> :: more_tagged(a,b)"#);

            let expected = Modifiers{ modifiers: vec![
                RsTestAttribute::attr("simple"),
                RsTestAttribute::tagged("tagged", vec!["first", "second"]),
                RsTestAttribute::typed("type_tag", "(u32, T, (std::string::String, i32))"),
                RsTestAttribute::tagged("more_tagged", vec!["a", "b"]),
            ] };

            assert_eq!(expected, modifiers);
        }
    }

    mod parse_test_case {
        use super::assert_eq;
        use super::*;

        fn parse_test_case<S: AsRef<str>>(test_case: S) -> TestCase {
            parse_meta(test_case)
        }

        #[test]
        fn two_literal_args() {
            let test_case = parse_test_case(r#"case(42, "value")"#);
            let args = test_case.args();

            let expected = to_args!(["42", r#""value""#]);

            assert_eq!(expected, args);
        }

        #[test]
        fn some_literals() {
            let args_expressions = vec!["42", "42isize", "1.0", "-1", "-1.0", "true",
                                        "1_000_000u64", "0b10100101u8", r#""42""#, "b'H'"];
            let test_case = parse_test_case(&format!("case({})", args_expressions.join(", ")));
            let args = test_case.args();

            assert_eq!(to_args!(args_expressions), args);
        }

        #[test]
        fn raw_code() {
            let test_case = parse_test_case(r#"case(vec![1,2,3])"#);
            let args = test_case.args();

            assert_eq!(to_args!(["vec![1, 2, 3]"]), args);
        }

        #[test]
        #[should_panic(expected = r#"Cannot parse due"#)]
        fn raw_code_with_parsing_error() {
            parse_test_case(r#"case(some:<>(1,2,3))"#);
        }

        #[test]
        fn should_read_test_description_if_any() {
            let test_case = parse_test_case(r#"case::this_test_description(42)"#);
            let args = test_case.args();

            assert_eq!("this_test_description", &test_case.description.unwrap().to_string());
            assert_eq!(to_args!(["42"]), args);
        }

        #[test]
        fn should_read_test_description_also_with_more_args() {
            let test_case = parse_test_case(r#"case :: this_test_description (42, 24)"#);
            let args = test_case.args();

            assert_eq!("this_test_description", &test_case.description.unwrap().to_string());
            assert_eq!(to_args!(["42", "24"]), args);
        }

        #[test]
        fn should_parse_arbitrary_rust_code_as_expression() {
            let test_case = parse_test_case(r##"
            case(42, -42,
            pippo("pluto"),
            Vec::new(),
            String::from(r#"prrr"#),
            {
                let mut sum=0;
                for i in 1..3 {
                    sum += i;
                }
                sum
            },
            vec![1,2,3]
            )"##);
            let args = test_case.args();

            assert_eq!(to_args!(["42", "-42", r#"pippo("pluto")"#, "Vec::new()",
            r##"String::from(r#"prrr"#)"##, r#"{let mut sum=0;for i in 1..3 {sum += i;}sum}"#,
            "vec![1,2,3]"]), args);
        }
    }

    mod parse_args_and_cases {
        use pretty_assertions::assert_eq;
        use super::*;

        fn parse_data<S: AsRef<str>>(test_case: S) -> ParametrizeData {
            parse_meta(test_case)
        }

        #[test]
        fn one_simple_case_one_arg() {
            let data = parse_data(r#"arg, case(42)"#);

            assert_eq!(1, data.args.len());
            assert_eq!(1, data.cases.len());
            assert_eq!("arg", &data.args[0].to_string());
            assert_eq!(to_args!(["42"]), data.cases[0].args())
        }

        #[test]
        fn happy_path() {
            let data = parse_data(r#"
                arg1, arg2, arg3,
                case(1,2,3),
                case(11,12,13),
                case(21,22,23)
                "#);

            assert_eq!(to_strs!(vec!["arg1", "arg2", "arg3"]), to_strs!(data.args));
            assert_eq!(3, data.cases.len());
            assert_eq!(to_args!(["1", "2", "3"]), data.cases[0].args());
            assert_eq!(to_args!(["11", "12", "13"]), data.cases[1].args());
            assert_eq!(to_args!(["21", "22", "23"]), data.cases[2].args());
        }

        #[test]
        fn should_accept_comma_at_the_end_of_cases() {
            let data = parse_data(r#"
                arg,
                case(42),
                "#);

            assert_eq!(1, data.args.len());
            assert_eq!(1, data.cases.len());
            assert_eq!("arg", &data.args[0].to_string());
            assert_eq!(to_args!(["42"]), data.cases[0].args())
        }
    }

}

