use magnum_opus::Decoder;
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use std::io::{self, Read, Write, Cursor};

/*
https://github.com/s1lentq/revoice/blob/master/revoice/src/VoiceEncoder_Opus.cpp#L173-L255

acc//unttänään klo 07.07
after 0x06 there's 2 bytes which are "total length"
then there's 2 bytes of frame length, if it's 0xffff then ignore
after these 2 bytes is your opus frame with said length
you have to iterate over total length until you reach zero
there can be multiple "frames"
i don't actually know the terms
ah yes if length is not zero, then there will be also 2 bytes of "sequence number", it can be ignored too
sequence number doesn't count towards frame length


edeilen klo 12.09
damn I always miss the good discussion
I think I would call them frames too. There's opus frames and the steam voice frames. Steam voice frames with type OPUSCODEC_PLC contain a data section which can be one or more opus frames, if that makes sense
https://github.com/Meachamp/gm_8bit/blob/master/templates/steam_voice.bt
I put the template I wrote to visualize all this on github there, so you can use that if you get stuck
*/

const MAX_CHANNELS: usize = 1;
const FRAME_SIZE: usize = 160;
const MAX_FRAME_SIZE: usize = 3 * FRAME_SIZE;
const MAX_PACKET_LOSS: usize = 10;

enum SteamVoiceOp {
    codec_opusplc = 6,
    samplerate = 11,
    unk = 10,
    silence = 0,
    codec_legacy = 1,
    codec_unk = 2,
    codec_raw = 3,
    codec_opus = 5,
    codec_silk = 4
}

pub struct OpusFrame {
    pub sample_rate: u16,
    pub data: Vec<u8>
}

pub fn decode(buffer: &[u8]) -> Vec<OpusFrame> {
    if buffer.len() < 8 {
        return Vec::new()
    }

    // last 4 bytes is CRC
    let mut input = Cursor::new(&buffer[0..buffer.len() - 4]);

    let _sid = input.read_u64::<LittleEndian>().unwrap();

    let mut frames = Vec::new();

    let mut sample_rate = 24000;

    loop {
        let op = match input.read_u8() {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            _ => panic!()
        };
        println!("processing op: {}", op);
        if op == SteamVoiceOp::samplerate as u8 {
            sample_rate = input.read_u16::<LittleEndian>().unwrap();
        } else if op == SteamVoiceOp::codec_opusplc as u8 {
            let data_len = input.read_u16::<LittleEndian>().unwrap();

            // if (nPayloadSize == 0xFFFF)
            // {
            // 	ResetState();
            // 	m_nDecodeSeq = 0;
            // 	break;
            // }
            println!("steam frame size: {}", data_len);
            let mut steam_frame = vec![0; data_len as usize];
            input.read_exact(&mut steam_frame).unwrap();

            let mut c = Cursor::new(steam_frame);

            loop {
                let frame_len = match c.read_i16::<LittleEndian>() {
                    Ok(l) => l,
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    _ => panic!()
                };
                if frame_len == -1 {
                    break;
                }

                let _seq = c.read_u16::<LittleEndian>().unwrap();
                let mut frame_data = vec![0; frame_len as usize];
                c.read_exact(&mut frame_data).unwrap();
                
                frames.push(OpusFrame {
                    sample_rate: sample_rate,
                    data: frame_data
                });
            }
        } else if op == SteamVoiceOp::codec_opus as u8 || op == SteamVoiceOp::codec_legacy as u8 || op == SteamVoiceOp::codec_silk as u8 {
            let data_len = input.read_u16::<LittleEndian>().unwrap();
            io::copy(&mut Read::by_ref(&mut input).take(data_len as u64), &mut io::sink());
        } else if op == SteamVoiceOp::codec_raw as u8 {
            break; // bruh
        } else if op == SteamVoiceOp::unk as u8 {
            input.read_u8().unwrap();
            input.read_u8().unwrap();
        } else if op == SteamVoiceOp::silence as u8 {
            let _silent_samples = input.read_u16::<LittleEndian>().unwrap();
        } else {
            println!("unknown op {}", op);
        }
    }

    frames
}

/// Decode Steam + decode Opus
pub fn process<W: Write>(buffer: &[u8], out: &mut W) {
    let frames = decode(buffer);

    let mut decoder = Decoder::new(24000, magnum_opus::Channels::Mono).unwrap();

    for frame in frames {
        let mut buffer = vec![0i16; MAX_FRAME_SIZE];
        let size = decoder.decode(&frame.data, &mut buffer[..], false).unwrap();
        println!("decoded {} from {}", size, &frame.data.len());
        
        let mut bytes_buffer = [0u8; MAX_FRAME_SIZE*2];
        LittleEndian::write_i16_into(&buffer[0..size], &mut bytes_buffer[0..size * 2]);
        out.write(&bytes_buffer[0..size * 2]).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opus() {
        let mut buf = std::fs::read("compressed.dat").unwrap();
        let mut fout = std::fs::File::create("decomp.dat").unwrap();
        process(&buf, &mut fout);
    }
}
