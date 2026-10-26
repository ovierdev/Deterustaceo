//! Crate `captura`: lee audio del micrófono a través de ALSA.

use alsa::pcm::{Access, Format, HwParams, PCM};
use alsa::{Direction, ValueOr};
use anyhow::{Context, Result, bail};

/// Parámetros que le pedimos al hardware.
pub struct ConfigCaptura {
    pub dispositivo: String,
    pub frecuencia: u32,
    pub canales: u32,
}

/// Un micrófono abierto y listo para leer.
pub struct Captura {
    pcm: PCM,
}

impl Captura {
    /// Abre el micrófono y lo configura. Falla con un mensaje claro si algo no cuadra.
    pub fn abrir(cfg: &ConfigCaptura) -> Result<Self> {
        let pcm = PCM::new(&cfg.dispositivo, Direction::Capture, false)
            .with_context(|| format!("no se pudo abrir el micrófono '{}'", cfg.dispositivo))?;

        // Bloque propio: `hwp` toma prestado `pcm` y debe soltarlo antes de devolverlo.
        {
            let hwp =
                HwParams::any(&pcm).context("no se pudieron leer los parámetros del hardware")?;
            hwp.set_access(Access::RWInterleaved)?;
            hwp.set_format(Format::s16())?; // S16 nativo = S16_LE en la Pi
            hwp.set_channels(cfg.canales)
                .with_context(|| format!("el micrófono no acepta {} canal(es)", cfg.canales))?;
            hwp.set_rate(cfg.frecuencia, ValueOr::Nearest)?;
            pcm.hw_params(&hwp)
                .context("el hardware rechazó la configuración")?;
        }

        // Seguridad: comprobar lo que REALMENTE nos dio el hardware.
        let real = pcm.hw_params_current()?.get_rate()?;
        if real != cfg.frecuencia {
            bail!(
                "se pidieron {} Hz pero el hardware da {} Hz",
                cfg.frecuencia,
                real
            );
        }

        Ok(Self { pcm })
    }

    /// Lee un bloque de muestras. Devuelve cuántas se leyeron.
    pub fn leer(&self, buf: &mut [i16]) -> Result<usize> {
        let io = self.pcm.io_i16()?;
        match io.readi(buf) {
            Ok(n) => Ok(n),
            Err(e) => {
                // "Overrun": no leímos a tiempo y se perdieron muestras. Avisar y recuperar.
                eprintln!("aviso: se perdieron muestras ({e}), recuperando…");
                self.pcm
                    .try_recover(e, true)
                    .context("no se pudo recuperar la captura")?;
                Ok(0)
            }
        }
    }
}
