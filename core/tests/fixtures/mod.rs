#![allow(clippy::unwrap_used)]

pub fn wav(title: &str, artist: &str, album: &str, samples: usize) -> Vec<u8> {
    let mut info = b"INFO".to_vec();
    for (key, value) in [(b"INAM", title), (b"IART", artist), (b"IPRD", album)] {
        if value.is_empty() {
            continue;
        }
        let mut data = value.as_bytes().to_vec();
        data.push(0);
        if data.len() % 2 == 1 {
            data.push(0);
        }
        info.extend_from_slice(key);
        info.extend_from_slice(&(data.len() as u32).to_le_bytes());
        info.extend_from_slice(&data);
    }

    let mut list = b"LIST".to_vec();
    list.extend_from_slice(&(info.len() as u32).to_le_bytes());
    list.extend_from_slice(&info);

    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&8000u32.to_le_bytes());
    fmt.extend_from_slice(&16000u32.to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());

    let mut fmt_chunk = b"fmt ".to_vec();
    fmt_chunk.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    fmt_chunk.extend_from_slice(&fmt);

    let pcm = vec![0u8; samples * 2];
    let mut data_chunk = b"data".to_vec();
    data_chunk.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    data_chunk.extend_from_slice(&pcm);

    let mut body = b"WAVE".to_vec();
    body.extend_from_slice(&fmt_chunk);
    body.extend_from_slice(&list);
    body.extend_from_slice(&data_chunk);

    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn write_song(
    dir: &std::path::Path,
    name: &str,
    title: &str,
    artist: &str,
    album: &str,
) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, wav(title, artist, album, 400)).unwrap();
    path
}

pub fn wav_untagged(samples: usize) -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&8000u32.to_le_bytes());
    fmt.extend_from_slice(&16000u32.to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());

    let mut fmt_chunk = b"fmt ".to_vec();
    fmt_chunk.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    fmt_chunk.extend_from_slice(&fmt);

    let pcm = vec![0u8; samples * 2];
    let mut data_chunk = b"data".to_vec();
    data_chunk.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    data_chunk.extend_from_slice(&pcm);

    let mut body = b"WAVE".to_vec();
    body.extend_from_slice(&fmt_chunk);
    body.extend_from_slice(&data_chunk);

    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn write_untagged_song(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, wav_untagged(400)).unwrap();
    path
}
