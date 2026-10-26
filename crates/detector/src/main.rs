use anyhow::{Context, Result};
use captura::{Captura, ConfigCaptura};

// Valores fijos por ahora: en la v0.2 vendran del TOML
const DISPOSITIVO: &str = "plughw:CARD=B100,DEV=0";
const FRECUENCIA: u32 = 16_000;
const SEGUNDOS: u32 = 3;
const SALIDA: &str = "grabacion.wav";

fn main() -> Result<()> {
    let cfg = ConfigCaptura {
        dispositivo: DISPOSITIVO.into(),
        frecuencia: FRECUENCIA,
        canales: 1,
    };

    let mic = Captura::abrir(&cfg)?;
    println!("Grabando {SEGUNDOS} s desde {DISPOSITIVO}...");

    let total = (FRECUENCIA * SEGUNDOS) as usize;
    let mut muestras: Vec<i16> = Vec::with_capacity(total);
    let mut bloque = [0i16; 1024];

    while muestras.len() < total {
        let n = mic.leer(&mut bloque)?;
        let faltan = total - muestras.len();
        muestras.extend_from_slice(&bloque[..n.min(faltan)]);
    }

    guardar_wav(SALIDA, &muestras, FRECUENCIA)?;
    informe(&muestras);
    println!("Guardado en {SALIDA}");
    Ok(())
}

fn guardar_wav(ruta: &str, muestras: &[i16], frecuencia: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: frecuencia,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w =
        hound::WavWriter::create(ruta, spec).with_context(|| format!("No se pudo crear {ruta}"))?;

    for &m in muestras {
        w.write_sample(m)?;
    }
    w.finalize()?;
    Ok(())
}

fn informe(muestras: &[i16]) {
    let pico = muestras.iter().map(|m| m.unsigned_abs()).max().unwrap_or(0);
    let saturadas = muestras.iter().filter(|&&m| m == i16::MIN).count();
    let dbfs = if pico == 0 {
        f32::NEG_INFINITY
    } else {
        20.0 * (pico as f32 / 32768.0).log10()
    };
    println!("Pico: {pico} ({dbfs:.1} dBFS) | muestras saturadas: {saturadas}");
}
