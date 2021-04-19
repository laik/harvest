pub trait Filter {
    fn pass(&self, message: &str) -> bool;
}

struct BaseFilter {}

impl Filter for BaseFilter {
    fn pass(&self, message: &str) -> bool {
        let _ = message;
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
