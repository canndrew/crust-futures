#[derive(Serialize, Deserialize)]
pub enum DiscoveryMsg<T> {
    Request,
    Response(T),
}

