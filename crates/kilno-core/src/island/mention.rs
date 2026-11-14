//! The text a file dropped on the island leaves in the new session's prompt.

/// `@name `, relative to the session's folder (which is the file's). Claude Code reads a quoted
/// `@"..."` for a name with a space in it. A trailing space so you can carry on typing.
pub fn mention(name: &str) -> String {
    if name.contains(' ') {
        format!("@\"{name}\" ")
    } else {
        format!("@{name} ")
    }
}

#[cfg(test)]
mod tests {
    use super::mention;

    #[test]
    fn plain_name() {
        assert_eq!(mention("notes.md"), "@notes.md ");
    }

    #[test]
    fn name_with_a_space_is_quoted() {
        assert_eq!(mention("my notes.md"), "@\"my notes.md\" ");
    }
}
