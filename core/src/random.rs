use std::{
    cell::Cell,
    fs::File,
    io::Read,
    time::{SystemTime, UNIX_EPOCH},
};

const BUFFER_BYTES: usize = 512;

#[cfg(unix)]
fn system_entropy() -> Option<File> {
    File::open("/dev/urandom").ok()
}

#[cfg(not(unix))]
fn system_entropy() -> Option<File> {
    None
}

pub struct Entropy {
    file: Option<File>,
    buffer: [u8; BUFFER_BYTES],
    position: usize,
    filled: usize,
}

impl Default for Entropy {
    fn default() -> Entropy {
        Entropy::new()
    }
}

impl Entropy {
    pub fn new() -> Entropy {
        Entropy {
            file: system_entropy(),
            buffer: [0u8; BUFFER_BYTES],
            position: 0,
            filled: 0,
        }
    }

    fn refill(&mut self) {
        let read = match self.file.as_mut() {
            Some(file) => file.read(&mut self.buffer).unwrap_or(0),
            None => 0,
        };
        if read == 0 {
            self.file = None;
            fill_from_fallback(&mut self.buffer);
            self.filled = BUFFER_BYTES;
        } else {
            self.filled = read;
        }
        self.position = 0;
    }

    fn next_byte(&mut self) -> u8 {
        if self.position >= self.filled {
            self.refill();
        }
        let byte = self.buffer.get(self.position).copied().unwrap_or(0);
        self.position += 1;
        byte
    }

    pub fn fill(&mut self, out: &mut [u8]) {
        for byte in out.iter_mut() {
            *byte = self.next_byte();
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        self.fill(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    pub fn below(&mut self, bound: u64) -> u64 {
        if bound <= 1 {
            return 0;
        }
        let zone = (u64::MAX / bound) * bound;
        loop {
            let value = self.next_u64();
            if value < zone {
                return value % bound;
            }
        }
    }
}

pub fn random_bytes<const N: usize>(out: &mut [u8; N]) {
    Entropy::new().fill(out);
}

fn fill_from_fallback(bytes: &mut [u8]) {
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0) };
    }
    STATE.with(|state| {
        if state.get() == 0 {
            state.set(seed());
        }
        for chunk in bytes.chunks_mut(8) {
            let mut x = state.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            state.set(x);
            for (out, byte) in chunk.iter_mut().zip(x.to_le_bytes()) {
                *out = byte;
            }
        }
    });
}

fn seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B97F4A7C15);
    nanos ^ u64::from(std::process::id()).wrapping_mul(0x9E3779B97F4A7C15)
}

pub fn shuffle<T>(slice: &mut [T]) {
    let mut entropy = Entropy::new();
    for i in (1..slice.len()).rev() {
        let j = entropy.below(i as u64 + 1) as usize;
        slice.swap(i, j);
    }
}
