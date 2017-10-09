use config_file_handler;
use priv_prelude::*;

quick_error! {
    /// Crust's universal error type.
    #[derive(Debug)]
    pub enum CrustError {
        /// Config file handling errors
        ConfigFileHandler(e: config_file_handler::Error) {
            description("Config file handling error")
            display("Config file handling error: {}", e)
            cause(e)
            from()
        }
        /// Wrapper for a `std::io::Error`
        Io(e: io::Error) {
            description("IO error")
            display("IO error: {}", e)
            cause(e)
            from()
        }
        BootstrapFailed {
            description("Bootstrap failed")
        }
        NatError(e: NatError) {
            description("Error from NAT library")
            display("Error from NAT library: {}", e)
            from()
        }
    }
}

