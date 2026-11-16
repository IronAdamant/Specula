//! A command's arguments: `--name value` and `--flag`, nothing else.

/// The arguments still to be read.
pub struct Args(Vec<String>);

impl Args {
    pub fn new(args: Vec<String>) -> Args {
        Args(args)
    }

    /// Whether `--name` was given (it is taken out).
    pub fn flag(&mut self, name: &str) -> bool {
        match self.0.iter().position(|a| a == name) {
            Some(i) => {
                self.0.remove(i);
                true
            }
            None => false,
        }
    }

    /// The value after `--name`, if it was given (both are taken out).
    pub fn value(&mut self, name: &str) -> Result<Option<String>, String> {
        let Some(i) = self.0.iter().position(|a| a == name) else { return Ok(None) };
        if i + 1 >= self.0.len() {
            return Err(format!("{name} takes a value"));
        }
        self.0.remove(i);
        Ok(Some(self.0.remove(i)))
    }

    /// `value`, as a whole number, or `default`.
    pub fn number(&mut self, name: &str, default: u64) -> Result<u64, String> {
        match self.value(name)? {
            Some(v) => v.parse().map_err(|_| format!("{name} takes a whole number, not {v:?}")),
            None => Ok(default),
        }
    }

    /// A comma-separated list after `--name` (empty when it was not given).
    pub fn list(&mut self, name: &str) -> Result<Vec<String>, String> {
        Ok(self.value(name)?.map(|v| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()).unwrap_or_default())
    }

    /// The words still unread, in order: a command's own words, once its flags are taken (a
    /// command whose words may be anything, such as a tool's arguments, takes no flags first).
    pub fn rest(self) -> Vec<String> {
        self.0
    }

    /// Fails when something was given that no one read.
    pub fn done(self) -> Result<(), String> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(format!("unknown argument(s): {}", self.0.join(" ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        Args::new(list.iter().map(|s| (*s).to_owned()).collect())
    }

    #[test]
    fn values_flags_and_what_nobody_read() {
        let mut a = args(&["--jobs", "4", "--no-gpu", "--only", "tests, clippy,", "--extra"]);
        assert_eq!(a.number("--jobs", 8), Ok(4));
        assert_eq!(a.number("--repeat", 5), Ok(5));
        assert!(a.flag("--no-gpu") && !a.flag("--no-gpu"));
        assert_eq!(a.list("--only").unwrap(), ["tests", "clippy"]);
        assert_eq!(a.done(), Err("unknown argument(s): --extra".to_owned()));
        assert!(args(&["--jobs"]).value("--jobs").is_err());
        assert!(args(&["--jobs", "many"]).number("--jobs", 8).is_err());
        assert!(args(&[]).done().is_ok());
    }
}
