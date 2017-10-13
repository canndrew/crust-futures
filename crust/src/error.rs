use notify;
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
        PeerNotFound {
            description("The requested peer was not found")
        }
        PeerError(e: PeerError) {
            description("error raised on a peer")
            display("error raised on a peer: {}", e)
            cause(e)
            from()
        }
        ConfigFileWatcher(e: notify::Error) {
            description("error starting config file watcher")
            display("error starting config file watcher: {}", e)
            cause(e)
            from()
        }
        PrepareConnectionInfo(e: io::Error) {
            description("error preparing connection info")
            display("error preparing connection info. {}", e)
        }
        StartListener(e: io::Error) {
            description("error starting listener")
            display("error starting listener, {}", e)
        }
    }
}

