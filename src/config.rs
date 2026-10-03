#[derive(Debug)]
pub struct Config {
    pub permanent_rm: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            permanent_rm: false,
        }
    }
}
