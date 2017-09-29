use std::net::SocketAddrV4;
use net2::TcpBuilder;
use tokio_core::reactor::Handle;
use tokio_igd::PortMappingProtocol;
use futures::{future, stream, Future, Stream};
use future_utils::{BoxFuture, StreamExt};
use log::LogLevel;
use void;

use nat::util;
use nat::{NatError, MappingContext};

/// Create a new, mapped tcp socket.
pub fn mapped_tcp_socket(
    handle: &Handle,
    mc: &MappingContext,
) -> BoxFuture<(TcpBuilder, Vec<SocketAddrV4>), NatError> {
    let try = || -> Result<_, NatError> {
        let socket = util::new_reusably_bound_tcp_socket(&addr!("0.0.0.0:0"))?;
        let addr = socket.local_addr()?;

        let mut mapped_addrs = mc.ifv4s()
            .iter()
            .map(|&(ip, _)| SocketAddrV4::new(ip, addr.port()))
            .collect::<Vec<_>>();

        let mut mapping_futures = Vec::new();

        for &(ref ip, ref gateway) in mc.ifv4s() {
            let gateway = match *gateway {
                Some(ref gateway) => gateway.clone(),
                None => continue,
            };
            let local_endpoint = SocketAddrV4::new(*ip, addr.port());
            let future = gateway.get_any_address(
                PortMappingProtocol::TCP,
                local_endpoint,
                0,
                "MaidSafeNat",
            ).map_err(NatError::IgdAddAnyPort);
            mapping_futures.push(future);
        }

        let mapping_futures = stream::futures_unordered(mapping_futures);
        Ok(mapping_futures
            .log_errors(LogLevel::Info, "mapping tcp socket")
            .map_err(|v| void::unreachable(v))
            .collect()
            .and_then(move |addrs| {
                mapped_addrs.extend(addrs);
                Ok((socket, mapped_addrs))
            }))
    };
    future::result(try()).flatten().boxed()
}

#[cfg(test)]
mod test {
    use super::*;

    use tokio_core::reactor::Core;
    use nat::MappingContext;

    #[test]
    fn test_mapped_tcp_socket() {
        let mut core = unwrap!(Core::new());
        let handle = core.handle();
        let res = core.run(MappingContext::new()
            .and_then(|mc| {
                mapped_tcp_socket(&handle, &mc)
                    .map_err(NatError::from)
            })
            .and_then(|(socket, addrs)| {
                println!("Tcp mapped socket addrs: {:?}", addrs);
                Ok(())
            })
        );
        unwrap!(res);
    }
}

