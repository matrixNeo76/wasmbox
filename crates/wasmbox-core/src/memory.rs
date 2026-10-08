/// Pacchetta un puntatore e una lunghezza in un singolo `u64`.
///
/// Layout: `(ptr << 32) | len`. Il valore viene restituito al guest come
/// `i64` dalla host function `ask` e come `u64` da `guest_run`.
pub fn pack_ptr_len(ptr: u32, len: u32) -> u64 {
    ((ptr as u64) << 32) | (len as u64)
}

/// Decodifica un valore pacchetto nelle sue componenti `(ptr, len)`.
pub fn unpack_ptr_len(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(ptr: u32, len: u32) {
        let packed = pack_ptr_len(ptr, len);
        assert_eq!(unpack_ptr_len(packed), (ptr, len));
    }

    #[test]
    fn round_trip_zero_zero() {
        round_trip(0, 0);
    }

    #[test]
    fn round_trip_one_max() {
        round_trip(1, u32::MAX);
    }

    #[test]
    fn round_trip_max_one() {
        round_trip(u32::MAX, 1);
    }

    #[test]
    fn round_trip_random_values() {
        // Valori deterministici ma sparsi (niente dipendenze esterne).
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..64 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let a = (seed >> 32) as u32;
            let b = seed as u32;
            round_trip(a, b);
        }
    }

    #[test]
    fn unpack_is_inverse_of_shift_layout() {
        assert_eq!(unpack_ptr_len(0), (0, 0));
        assert_eq!(unpack_ptr_len((0xABCDu64 << 32) | 0x1234), (0xABCD, 0x1234));
    }
}
