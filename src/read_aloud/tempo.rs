//! Sonic changes speech tempo in the time domain, preserving consonants and pitch.
use libloading::Library;
use std::ffi::c_void;

type Stream = *mut c_void;
type Destroy = unsafe extern "C" fn(Stream);
type SetSpeed = unsafe extern "C" fn(Stream, f32);
type Write = unsafe extern "C" fn(Stream, *const i16, i32) -> i32;
type Read = unsafe extern "C" fn(Stream, *mut i16, i32) -> i32;
type Flush = unsafe extern "C" fn(Stream) -> i32;

pub(super) struct Tempo {
    stream: Stream,
    destroy: Destroy,
    set_speed: SetSpeed,
    write: Write,
    read: Read,
    flush: Flush,
    _library: Library,
}

impl Tempo {
    pub fn new(rate: i32, speed: f32) -> Result<Self, String> {
        // The function signatures below are Sonic's public C API; the library stays alive
        // until after its stream is destroyed. Buffers are aligned, bounded i16 slices.
        unsafe {
            let library = [
                "libsonic.so.0",
                "libsonic.so",
                "libsonic.dylib",
                "sonic.dll",
            ]
            .iter()
            .find_map(|name| Library::new(name).ok())
            .ok_or("Speech speed adjustment needs the Sonic library.")?;
            let create = *library
                .get::<unsafe extern "C" fn(i32, i32) -> Stream>(b"sonicCreateStream\0")
                .map_err(|error| error.to_string())?;
            let destroy = *library
                .get::<Destroy>(b"sonicDestroyStream\0")
                .map_err(|e| e.to_string())?;
            let set_speed = *library
                .get::<SetSpeed>(b"sonicSetSpeed\0")
                .map_err(|e| e.to_string())?;
            let write = *library
                .get::<Write>(b"sonicWriteShortToStream\0")
                .map_err(|e| e.to_string())?;
            let read = *library
                .get::<Read>(b"sonicReadShortFromStream\0")
                .map_err(|e| e.to_string())?;
            let flush = *library
                .get::<Flush>(b"sonicFlushStream\0")
                .map_err(|e| e.to_string())?;
            let quality = *library
                .get::<unsafe extern "C" fn(Stream, i32)>(b"sonicSetQuality\0")
                .map_err(|error| error.to_string())?;
            let stream = create(rate, 1);
            if stream.is_null() {
                return Err("Cannot create the speech speed processor.".into());
            }
            quality(stream, 1);
            set_speed(stream, speed);
            Ok(Self {
                stream,
                destroy,
                set_speed,
                write,
                read,
                flush,
                _library: library,
            })
        }
    }

    pub fn speed(&mut self, speed: f32) {
        unsafe { (self.set_speed)(self.stream, speed) };
    }

    fn drain(&mut self) -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        let mut buffer = [0_i16; 8192];
        loop {
            let count =
                unsafe { (self.read)(self.stream, buffer.as_mut_ptr(), buffer.len() as i32) };
            if count == 0 {
                return Ok(output);
            }
            if count < 0 || count as usize > buffer.len() {
                return Err("The speech speed processor returned invalid audio.".into());
            }
            output.extend(
                buffer[..count as usize]
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes()),
            );
        }
    }

    pub fn process(&mut self, pcm: &[u8]) -> Result<Vec<u8>, String> {
        if !pcm.len().is_multiple_of(2) {
            return Err("The speech stream contains an incomplete sample.".into());
        }
        let samples: Vec<i16> = pcm
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let count = i32::try_from(samples.len()).map_err(|error| error.to_string())?;
        if unsafe { (self.write)(self.stream, samples.as_ptr(), count) } == 0 {
            return Err("Speech speed processing ran out of memory.".into());
        }
        self.drain()
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, String> {
        if unsafe { (self.flush)(self.stream) } == 0 {
            return Err("Cannot finish speech speed processing.".into());
        }
        self.drain()
    }
}

impl Drop for Tempo {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.stream) };
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn faster_speech_keeps_a_sharp_transient_sharp() {
        for speed in [1.25, 1.4, 2.0] {
            let mut samples = vec![0_i16; 48_000];
            samples[12_000] = 24_000;
            let input: Vec<u8> = samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes())
                .collect();
            let mut tempo = Tempo::new(24_000, speed).unwrap();
            let mut output = tempo.process(&input).unwrap();
            output.extend(tempo.finish().unwrap());
            let output: Vec<i16> = output
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            let peak = output
                .iter()
                .map(|sample| sample.unsigned_abs())
                .max()
                .unwrap();
            assert!(peak > 1000);
            let active = output
                .iter()
                .filter(|sample| f32::from(sample.unsigned_abs()) > f32::from(peak) * 0.001)
                .count();
            assert!(
                active <= 4,
                "{speed}x spread one sharp sample into {active} samples"
            );
            let seconds = output.len() as f32 / 24_000.0;
            assert!((seconds - 2.0 / speed).abs() < 0.03);
        }
    }
}
