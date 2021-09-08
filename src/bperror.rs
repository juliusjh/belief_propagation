use crate::NodeIndex;

/*
BPError allows to create something like a stack trace enriched with debug infos.

This is much harder to maintain in belief_propagation, but as we use belief_propagation
mainly from python, with optimization (otherwise it's extremely slow), it's used
mostly by non Rust developers, and Rust backtraces from python with --release are
almost useless, this saves a lot of time in total.
*/

pub type BPResult<T> = Result<T, BPError>;

#[derive(Debug)]
pub struct BPError {
    function_name: Vec<String>,
    cause: Vec<String>,
    debug_info: Vec<String>,
}

impl std::fmt::Display for BPError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        assert!(!self.function_name.is_empty());
        assert_eq!(self.cause.len(), self.function_name.len());
        write!(f, "\"{}\" in {}", self.cause[0], self.function_name[0])?;
        for (name, cause) in self.function_name.iter().zip(self.cause.iter()) {
            write!(f, "\n\t-> {}: {}", name, cause)?;
        }

        #[cfg(feature = "debug_info_on_error")]
        {
            if self.debug_info.len() > 0 {
                writeln!(f, "\nDebug info:")?;
                for dbg_info in &self.debug_info {
                    writeln!(f, "{}", dbg_info)?;
                }
            }
        }
        Ok(())
    }
}

impl BPError {
    pub fn new(function_name: String, cause: String) -> Self {
        Self {
            function_name: vec![function_name],
            cause: vec![cause],
            debug_info: Vec::new(),
        }
    }
    //TODO: make function name static str
    pub fn new_with_debug(function_name: String, cause: String, debug_info: String) -> Self {
        Self {
            function_name: vec![function_name],
            cause: vec![cause],
            debug_info: vec![debug_info],
        }
    }
    pub fn attach_info(mut self, function_name: String, cause: String) -> Self {
        self.function_name.push(function_name);
        self.cause.push(cause);
        self
    }
    pub fn attach_info_str(mut self, function_name: &'static str, cause: String) -> Self {
        self.function_name.push(function_name.to_owned());
        self.cause.push(cause);
        self
    }
    pub fn attach_debug_info(mut self, debug_info: String) -> Self {
        self.debug_info.push(debug_info);
        self
    }
    pub fn attach_debug_object<T: std::fmt::Debug>(
        mut self,
        name: &'static str,
        object: T,
    ) -> Self {
        self.attach_debug_info(format!("{}: {:?}", name, object))
    }
}

impl std::error::Error for BPError {}
