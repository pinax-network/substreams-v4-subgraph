#[allow(clippy::all)]
pub mod pool_manager {
    include!(concat!(env!("OUT_DIR"), "/pool_manager.rs"));
}

#[allow(clippy::all)]
pub mod position_manager {
    include!(concat!(env!("OUT_DIR"), "/position_manager.rs"));
}

#[allow(clippy::all)]
pub mod arrakis_hook_factory {
    include!(concat!(env!("OUT_DIR"), "/arrakis_hook_factory.rs"));
}
