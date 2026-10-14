//! FIPS 202 §4 and §6: the sponge construction over KECCAK-p[1600, 24] with `pad10*1`.
//!
//! ## Standards ownership
//!
//! §4 Algorithm 8 absorbs the message in `r`-bit blocks after padding, then squeezes `r`-bit
//! blocks until the output is long enough. §5.1 defines `pad10*1`. §6.1 appends the domain
//! suffix `01` to SHA-3 messages and §6.2 appends `1111` to SHAKE messages before padding; with
//! byte-aligned input those suffixes plus the first padding bit are the single bytes `0x06` and
//! `0x1f`, and the final padding bit is `0x80` on the last byte of the block.
//!
//! The sponge is generic over the rate and suffix so SHA3-256 (`r = 1088`) and SHAKE256
//! (`r = 1088`) share one absorb/squeeze implementation.

use zeroize::Zeroize;

use super::keccak::State;

/// Absorbing and squeezing over one `State` with a fixed byte rate and domain suffix.
pub(super) struct Sponge<const RATE: usize> {
    state: State,
    /// Bytes of the current block already `XORed` in (absorbing) or already read (squeezing).
    position: usize,
    suffix: u8,
    squeezing: bool,
}

impl<const RATE: usize> Sponge<RATE> {
    pub(super) const fn new(suffix: u8) -> Self {
        Self {
            state: State::new(),
            position: 0,
            suffix,
            squeezing: false,
        }
    }

    /// Algorithm 8 steps 1–6 for the message bytes seen so far: XOR into the block and permute
    /// after every `RATE` bytes.
    pub(super) fn absorb(&mut self, mut input: &[u8]) {
        debug_assert!(!self.squeezing, "absorbing after squeezing is not defined");
        while !input.is_empty() {
            let take = (RATE - self.position).min(input.len());
            let mut block = [0_u8; 200];
            block[self.position..self.position + take].copy_from_slice(&input[..take]);
            self.state.absorb_bytes(&block[..RATE]);
            block.zeroize();
            self.position += take;
            input = &input[take..];
            if self.position == RATE {
                self.state.permute();
                self.position = 0;
            }
        }
    }

    /// §5.1 `pad10*1` with the §6 domain suffix, then the final absorbing permutation.
    fn finish_absorbing(&mut self) {
        let mut block = [0_u8; 200];
        block[self.position] = self.suffix;
        block[RATE - 1] |= 0x80;
        self.state.absorb_bytes(&block[..RATE]);
        self.state.permute();
        self.position = 0;
        self.squeezing = true;
    }

    /// Algorithm 8 steps 7–10: read output blocks, permuting between them.
    pub(super) fn squeeze(&mut self, mut output: &mut [u8]) {
        if !self.squeezing {
            self.finish_absorbing();
        }
        while !output.is_empty() {
            if self.position == RATE {
                self.state.permute();
                self.position = 0;
            }
            let take = (RATE - self.position).min(output.len());
            let mut block = [0_u8; 200];
            self.state.squeeze_bytes(&mut block[..RATE]);
            output[..take].copy_from_slice(&block[self.position..self.position + take]);
            block.zeroize();
            self.position += take;
            output = &mut output[take..];
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    /// Standard-derived evidence: the empty SHA-3 message pads to `06 00 … 80` (NIST's printed
    /// "Data to be absorbed" for the 0-bit message).
    #[test]
    fn empty_sha3_message_pads_to_the_printed_block() {
        let mut expected = [0_u8; 136];
        expected[0] = 0x06;
        expected[135] = 0x80;
        let mut sponge = Sponge::<136>::new(0x06);
        let mut padded = [0_u8; 200];
        padded[0] = 0x06;
        padded[135] |= 0x80;
        // Absorbing the padded block directly must equal finish_absorbing on an empty input.
        let mut direct = State::new();
        direct.absorb_bytes(&padded[..136]);
        direct.permute();
        let mut via_sponge = [0_u8; 32];
        sponge.squeeze(&mut via_sponge);
        let mut via_direct = [0_u8; 32];
        direct.squeeze_bytes(&mut via_direct);
        assert_eq!(via_sponge, via_direct);
        assert_eq!(&padded[..136], &expected);
    }
}
