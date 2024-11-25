// pub(crate) mod init;

pub(crate) fn any_in_collection<T, I>(collection: I, placeholder: &str) -> String
where
    T: std::fmt::Display,
    I: IntoIterator<Item = T>,
{
    let mut iter = collection.into_iter().peekable();
    let mut any = String::new();

    while let Some(item) = iter.next() {
        any.push_str(&format!("{}", item));
        if iter.peek().is_some() {
            any.push_str(placeholder);
        }
    }

    any
}
