pub struct CoreApi {
    pub value: u64,
}

pub fn public_core_value() -> u64 {
    private_core_value()
}

fn private_core_value() -> u64 {
    42
}
