use tokio_igd::PortMappingProtocol;
use log::LogLevel;
use util;
use void;

use priv_prelude::*;

/// Create a new, mapped tcp socket.
pub fn mapped_tcp_socket(
    mc: &MappingContext,
    addr: &SocketAddr,
) -> BoxFuture<(TcpBuilder, Vec<SocketAddr>), NatError> {
    let try = || -> Result<_, NatError> {
        let socket = util::new_reusably_bound_tcp_socket(addr)?;
        let addr = socket.local_addr()?;

        let mut mapped_addrs = mc.ifv4s()
            .iter()
            .map(|ifv4| SocketAddr::V4(SocketAddrV4::new(ifv4.ip(), addr.port())))
            .collect::<Vec<_>>();

        let mut mapping_futures = Vec::new();

        for ifv4 in mc.ifv4s() {
            let gateway = match ifv4.gateway() {
                Some(gateway) => gateway.clone(),
                None => continue,
            };
            let local_endpoint = SocketAddrV4::new(ifv4.ip(), addr.port());
            let future = gateway.get_any_address(
                PortMappingProtocol::TCP,
                local_endpoint,
                0,
                "MaidSafeNat",
            )
            .map(SocketAddr::V4)
            .map_err(NatError::IgdAddAnyPort);
            mapping_futures.push(future);
        }

        // TODO: stun peers, and use force_include_port

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
    future::result(try()).flatten().into_boxed()
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
