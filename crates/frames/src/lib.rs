pub fn crate_ready() -> bool { true }

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_builds() { assert!(super::crate_ready()); }
}
