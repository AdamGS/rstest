pub struct Magic<T>(pub core::marker::PhantomData<T>);

pub trait ViaParseDebug<'a, T> {
    fn magic_conversion(&self, input: &'a str) -> T;
}

impl<'a, T> ViaParseDebug<'a, T> for &&Magic<T>
where
    T: core::str::FromStr,
    T::Err: core::fmt::Debug,
{
    fn magic_conversion(&self, input: &'a str) -> T {
        T::from_str(input).unwrap()
    }
}

pub trait ViaParse<'a, T> {
    fn magic_conversion(&self, input: &'a str) -> T;
}

impl<'a, T> ViaParse<'a, T> for &Magic<T>
where
    T: core::str::FromStr,
{
    fn magic_conversion(&self, input: &'a str) -> T {
        match T::from_str(input) {
            Ok(v) => v,
            Err(_) => {
                panic!(
                    "Cannot parse '{}' to get {}",
                    input,
                    core::any::type_name::<T>()
                );
            }
        }
    }
}

pub trait ViaIdent<'a, T> {
    fn magic_conversion(&self, input: &'a str) -> T;
}

impl<'a> ViaIdent<'a, &'a str> for &&Magic<&'a str> {
    fn magic_conversion(&self, input: &'a str) -> &'a str {
        input
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use core::str::FromStr;

    #[test]
    fn should_return_the_same_slice_string() {
        assert_eq!(
            "something",
            (&&&Magic::<&str>(core::marker::PhantomData)).magic_conversion("something")
        );
    }

    #[test]
    fn should_parse_via_parse_debug() {
        assert_eq!(
            42u32,
            (&&&Magic::<u32>(core::marker::PhantomData)).magic_conversion("42")
        );
    }

    #[test]
    fn should_parse_via_parse_no_error_debug() {
        struct S(String);
        struct E;
        impl FromStr for S {
            type Err = E;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(S(s.to_owned()))
            }
        }

        assert_eq!(
            "some",
            (&&&Magic::<S>(core::marker::PhantomData))
                .magic_conversion("some")
                .0
        );
    }

    #[test]
    #[should_panic(expected = "MyTypeName")]
    fn should_show_error() {
        struct MyTypeName;
        struct E;
        impl FromStr for MyTypeName {
            type Err = E;

            fn from_str(_s: &str) -> Result<Self, Self::Err> {
                Err(E)
            }
        }
        (&&&Magic::<MyTypeName>(core::marker::PhantomData)).magic_conversion("");
    }
}
