/// The Allwinner DRAM-init blob, bundled (small, redistributable boot tooling).
#[allow(dead_code)] // consumed by memboot command in a later task
const FES1: &[u8] = include_bytes!("../../resources/fes1.bin");

/// The bundled fes1 DRAM-init blob.
#[allow(dead_code)] // consumed by memboot command in a later task
pub fn fes1() -> &'static [u8] {
    FES1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fes1_is_present_and_nontrivial() {
        assert!(fes1().len() > 1024, "fes1 blob looks too small");
    }
}
