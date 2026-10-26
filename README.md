# Detector de Audio de Seguridad - Guia del Proyecto

Detector que escucha con un micrófono, identifica con IA si un sonido es **vidrio roto** o un **disparo**, activa una alarma local y publica un evento  para app. 

__ALCANCE__: este proyecto es solo l aparte de hardware/sensor. 


| Dato | Valor |
|---|---|
| Hardware | Raspberry Pi 4 + micrófono USB + LED |
| Sistema | Raspberry Pi OS Lite (64-bit), en una SD distinta a la del SO bare-metal |
| Lenguaje | Rust (binario compilado `aarch64-unknown-linux-gnu`) |
| IA | Entrenamiento en PC (Python) → modelo ONNX → inferencia en Rust en la Pi |
| Versión actual | `0.0.0` (planificación) |

## Decisiones Tomadas  
| Tema | Decisión | Por qué |
|---|---|---|
| Lenguaje | **Rust** | Compilado, seguridad de memoria, sin recolector de basura, buen ecosistema (audio, FFT, ONNX). Refuerza lo aprendido en otros proyectos. |
| Entrenamiento IA | **Python en el PC** (PyTorch o TensorFlow) | El ecosistema de ML está ahí. Python solo es herramienta de laboratorio, no va al producto. |
| Formato del modelo | **ONNX** | Estándar abierto; se ejecuta desde Rust con `tract` u `ort`. |
| Micrófono | **USB** (para empezar) | Funciona sin configuración extra. Más adelante: MEMS I2S (INMP441). |
| Salida local | **Alarma: LED por GPIO** (más adelante sirena/relé) | Aviso inmediato en el lugar, aunque falle la red. |
| Salida remota | **Evento JSON a la app de seguridad** | La app se encarga de avisar al móvil; el detector solo actúa como sensor. |
| Integración con la app | **Contrato de interfaz** versionado (sección 4) | Cada parte trabaja sin bloquear a la otra. |
| GPIO | Crate **`gpiocdev`** | `rppal` fue archivado en julio de 2025; `gpiocdev` usa la interfaz estándar del kernel (`/dev/gpiochip0`). |
| Configuración | Archivo **TOML** + **asistente CLI** | La config es la fuente de verdad; nada "quemado" en el código. |
| Compilación | Primero **nativa en la Pi**; compilación cruzada después | Menos fricción al empezar. |



### Pendiente de decidir
- [ ] **Transporte hacia la app:** MQTT (recomendado) o HTTP webhook. Hay que acordarlo con quien desarrolla la app.
- [ ] **¿Qué app es y cómo recibe señales?** (p. ej. Frigate usa MQTT, MotionEye usa webhooks, o app propia).
- [ ] Aprobación del contrato JSON (sección 4) por parte del equipo de la app.
Se diseña como trait `Notificador`: cambiar de transporte no toca el código de detección.
 
---
 
## 2. Arquitectura
 
```
Micrófono USB → Captura (16 kHz, mono) → Buffer circular (~1 s)
   → Etapa 1: Disparador barato (energía / onset)      ← corre siempre, casi gratis
   → Etapa 2: Espectrograma log-Mel (64 bandas)
   → Etapa 3: Red neuronal CNN pequeña → {vidrio, disparo, otro}
   → Etapa 4: Lógica de decisión (umbral de confianza + confirmaciones)
   → Salidas: Alarma LED (GPIO) + Evento a la app (Notificador) + log
```
 
### Límite del sistema
```
[ Este proyecto: detector en Rust ]               [ Otro equipo: app de seguridad ]
  Micrófono → IA → decisión ──► Alarma LED
                            └──► Evento JSON ────► app ──► alerta al móvil
                            └──► Latido (heartbeat) ──► app ──► "detector caído"
```
 
La Etapa 1 hace que la IA solo se despierte ante un sonido fuerte y súbito: ahorra CPU y reduce falsas alarmas.
 
---
 
## 3. Arranque y configuración
 
### Subcomandos (`clap`)
```
detector [run]    → arranca normal (por defecto)
detector init     → abre el asistente aunque ya exista config
detector check    → valida config y prueba el hardware, sin detectar
```
 
### Búsqueda de la configuración (en orden)
1. `--config ruta/al/archivo.toml`
2. `./detector.toml`
3. `/etc/detector-audio/config.toml`
### Flujo de arranque
```
¿Existe config? ── sí ──► validar ──► ¿válida? ── sí ──► ejecutar
      │                                  │
      no                                 no ──► error claro + salir (código ≠ 0)
      │
¿Hay terminal interactiva (TTY)?
      ├── sí ──► asistente CLI ──► guarda config ──► ejecutar
      └── no ──► error claro en log + salir (código ≠ 0)
```
 
### Reglas de seguridad de la configuración
1. **Sin TTY no hay asistente.** Bajo `systemd` nadie puede responder; el programa no debe quedarse colgado. Falla de forma *ruidosa*.
2. **Secretos fuera del TOML.** Tokens y contraseñas van en un archivo aparte con permisos `600` o en variables de entorno. Al repositorio solo sube `detector.example.toml`.
3. **`#[serde(deny_unknown_fields)]`.** Un campo mal escrito impide arrancar en vez de ignorarse en silencio.
4. **El asistente prueba lo que configura:**
   - Micrófono: lista dispositivos, graba 3 s y muestra un medidor de nivel.
   - Alarma: hace parpadear el LED del pin elegido.
   - App: envía un evento de tipo `prueba` (ver sección 4).
### Ejemplo: `detector.example.toml`
```toml
version = 1                      # versión del esquema, para migraciones futuras
 
[audio]
dispositivo = "USB PnP Sound Device"
frecuencia_muestreo = 16000
canales = 1
 
[deteccion]
modelo = "modelos/model-v0.1.onnx"
umbral_energia_db = -30.0        # Etapa 1: disparador
umbral_confianza = 0.80          # Etapa 3: la IA debe estar ≥ 80 % segura
confirmaciones = 2               # ventanas consecutivas para confirmar
 
[dispositivo]
id = "pi-sala-01"                # identifica este detector ante la app
 
[salidas.alarma]
habilitado = true
chip = "/dev/gpiochip0"
pin_led = 17
duracion_alarma_s = 10
 
[salidas.app]
habilitado = true
transporte = "mqtt"              # o "http" — pendiente de acordar con la app
destino = "mqtts://localhost:8883"   # broker MQTT, o URL del webhook si es HTTP
tema = "detector/pi-sala-01/eventos" # solo MQTT
secretos = "/etc/detector-audio/secretos.toml"
 
[salud]
heartbeat_s = 60
```
 
---
 
## 4. Contrato de interfaz con la app
 
> Documento de acuerdo entre este proyecto (sensor) y la app de seguridad. Cualquier cambio sube el campo `version` y se comunica al equipo de la app.
> Estado: **propuesta v1**, pendiente de aprobación.
 
### Mensaje de detección
```json
{
  "version": 1,
  "tipo": "deteccion",
  "clase": "disparo",
  "confianza": 0.93,
  "timestamp": "2026-10-24T11:05:12+03:00",
  "dispositivo": "pi-sala-01",
  "modelo": "model-v0.1"
}
```
 
### Mensaje de latido (heartbeat)
```json
{ "version": 1, "tipo": "latido", "dispositivo": "pi-sala-01", "estado": "ok" }
```
Se envía cada `heartbeat_s` segundos. Si la app deja de recibirlo, debe avisar al móvil de que **el detector no responde**.
 
### Mensaje de prueba
```json
{ "version": 1, "tipo": "prueba", "dispositivo": "pi-sala-01" }
```
Lo envía `detector init` / `detector check`. La app no debe tratarlo como alarma real.
 
### Campos
 
| Campo | Tipo | Valores | Descripción |
|---|---|---|---|
| `version` | entero | `1` | Versión del contrato |
| `tipo` | texto | `deteccion`, `latido`, `prueba` | Clase de mensaje |
| `clase` | texto | `vidrio`, `disparo` | Solo en `deteccion` |
| `confianza` | número | `0.0`–`1.0` | Seguridad de la IA |
| `timestamp` | texto | ISO 8601 con zona horaria | Momento de la detección |
| `dispositivo` | texto | p. ej. `pi-sala-01` | Qué detector envía |
| `modelo` | texto | p. ej. `model-v0.1` | Versión del modelo que decidió |
| `estado` | texto | `ok`, `error_microfono`, ... | Solo en `latido` |
 
### Transporte (pendiente de acordar)
 
| Opción | Cómo funciona | Ventaja |
|---|---|---|
| **MQTT** (recomendado) | Se publica en el tema `detector/<id>/eventos`; la app se suscribe | **Last Will**: si el detector se cae, el broker avisa a la app automáticamente. |
| **HTTP webhook** | `POST` del JSON a una URL de la app | Lo más simple de entender y probar con `curl`. |
 
---
 
## 5. Seguridad
 
### Seguridad funcional (que no falle cuando importa)
- Servicio `systemd` con `Restart=always` y watchdog.
- **Heartbeat** periódico hacia la app: si el detector muere o el micrófono se desconecta, la app avisa al móvil (*fail-safe*).
- La **alarma local no depende de la red**: si la app o internet fallan, el LED se activa igual.
- Balance explícito entre **falso negativo** (no detectar un disparo: grave) y **falso positivo** (alarma falsa: molesta). En seguridad se aceptan más falsos positivos.
- Nada de `unwrap()` en producción: todos los errores se manejan.
### Seguridad informática
- Procesamiento local; **no** se graba audio continuo (privacidad).
- El servicio corre con un usuario sin privilegios (miembro de los grupos `audio` y `gpio`), nunca como root.
- Comunicación saliente cifrada (TLS).
---
 
## 6. Versionado
 
### Código
- **Git**, repositorio en GitHub o GitLab.
- **SemVer** `MAJOR.MINOR.PATCH`. Empezamos en `0.1.0`; `1.0.0` cuando sea confiable.
- **Conventional Commits**: `feat: ...`, `fix: ...`, `docs: ...`, `refactor: ...`, `test: ...`
- Etiquetas por versión: `git tag v0.1.0`
- `CHANGELOG.md` actualizado en cada versión.
- Ramas: `main` siempre estable; trabajo nuevo en `feat/<nombre>`.
### Modelo y datos
- Cada modelo lleva su versión en el nombre: `model-v1.2.onnx`.
- El binario registra en el log qué modelo cargó.
- Modelos y datasets pesados con **Git LFS** o **DVC**, nunca en Git normal.
- Cada modelo se documenta con: dataset usado, métricas (precisión, recall, F1) y fecha.
---
 
## 7. Estructura del repositorio
 
```
detector-audio/
├── Cargo.toml                   # workspace
├── crates/
│   ├── config/                  # TOML, validación, asistente CLI
│   ├── captura/                 # audio desde ALSA
│   ├── dsp/                     # FFT, Mel, energía
│   ├── inferencia/              # carga ONNX y clasifica
│   ├── salidas/                 # alarma LED (gpiocdev) + trait Notificador (evento a la app)
│   └── detector/                # binario principal
├── entrenamiento/               # Python: notebooks y scripts (solo PC)
├── modelos/                     # .onnx versionados (LFS)
├── docs/
│   ├── 01-teoria-sonido-digital.md   # muestreo, Nyquist, PCM, S16_LE
│   ├── 02-teoria-espectro.md         # FFT, ventanas, STFT, escala Mel
│   ├── 03-teoria-deteccion.md        # onset, energía, umbrales, firmas acústicas
│   ├── 04-teoria-ia.md               # CNN, datasets, métricas
│   ├── 05-arquitectura.md
│   ├── 06-seguridad.md
│   └── 07-contrato-app.md            # contrato de interfaz con la app
├── detector.example.toml
├── CHANGELOG.md
└── README.md
```
 
---
 
## 8. Hoja de ruta
 
| Versión | Hito | Estado |
|---|---|---|
| v0.1 | Repositorio + workspace + captura de audio y guardar un WAV | ⬜ |
| v0.2 | Config TOML + asistente CLI (`init`, `check`) | ⬜ |
| v0.3 | Medidor de nivel en tiempo real + disparador por energía | ⬜ |
| v0.4 | Espectrograma log-Mel en Rust (validado contra Python) | ⬜ |
| v0.5 | Entrenar la CNN en el PC con datasets públicos | ⬜ |
| v0.6 | Inferencia ONNX en Rust en la Pi | ⬜ |
| v0.7 | Lógica de decisión + alarma LED + evento a la app (contrato v1) | ⬜ |
| v0.8 | Robustez: systemd, watchdog, heartbeat, logs | ⬜ |
| v1.0 | Pruebas en campo, métricas reales, ajuste de umbrales | ⬜ |
 
---
 
## 9. Lección 1 — Preparar el terreno
 
### Paso 1: tarjeta SD
- Usar una SD **distinta** a la del SO bare-metal.
- Grabar **Raspberry Pi OS Lite (64-bit)** con Raspberry Pi Imager.
- En las opciones: activar SSH, configurar Wi-Fi y usuario.
### Paso 2: circuito del LED
```
Pin físico 11 (GPIO17) ──► resistencia 330 Ω ──► pata larga del LED (ánodo)
Pin físico 9  (GND)    ◄─────────────────────── pata corta (cátodo)
```
 
### Paso 3: Rust en la Pi
```bash
ssh usuario@raspberrypi.local
sudo apt update && sudo apt install -y git build-essential libasound2-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
`libasound2-dev` = cabeceras de ALSA (sistema de audio de Linux).
 
### Paso 4: verificar el micrófono
```bash
arecord -l                                            # lista dispositivos de captura
arecord -D plughw:1,0 -f S16_LE -r 16000 -d 3 prueba.wav
aplay prueba.wav                                      # o copiarlo al PC para oírlo
```
 
### Tarea
- [ ] SD preparada y acceso por SSH
- [ ] LED cableado
- [ ] Rust instalado (`rustc --version`)
- [ ] Salida de `arecord -l` enviada al profesor
**Siguiente:** crear el repositorio y el workspace, y escribir `docs/01-teoria-sonido-digital.md`.
 
---
 
## 10. Crates previstos
 
| Crate | Uso |
|---|---|
| `clap` | Subcomandos y argumentos |
| `serde` + `toml` | Leer y validar la configuración |
| `dialoguer` | Preguntas interactivas del asistente |
| `cpal` (o `alsa`) | Captura de audio |
| `hound` | Leer/escribir WAV |
| `rustfft` | FFT para el espectrograma |
| `tract-onnx` (o `ort`) | Inferencia del modelo |
| `gpiocdev` | Alarma (LED) |
| `serde_json` | Mensajes del contrato con la app |
| `rumqttc` (o `reqwest`) | Transporte MQTT (o HTTP) hacia la app |
| `tracing` | Logs |
 
---
 
## 11. Datasets y lecturas
 
### Datasets
- **ESC-50**: incluye la clase *glass breaking*.
- **UrbanSound8K**: incluye la clase *gun shot*.
- **AudioSet** (Google): incluye ambas, a gran escala.
### Lecturas
- *The Scientist and Engineer's Guide to Digital Signal Processing* — Steven W. Smith (gratuito en línea). FFT y filtros.
- *Think DSP* — Allen Downey (gratuito).
- *Programming Rust* — Blandy, Orendorff y Tindall (O'Reilly).
- Gemmeke et al., 2017, *Audio Set: An ontology and human-labeled dataset for audio events*. Base de YAMNet y de la clasificación de audio con CNN.
---
 
## 12. Bitácora
 
| Fecha | Qué se hizo |
|---|---|
| 2026-10-24 | Planificación: lenguaje, arquitectura, versionado, configuración TOML + asistente, Lección 1 definida |
| 2026-10-24 | Alcance aclarado: este proyecto es solo el sensor. LED = alarma local; internet = evento JSON a la app de seguridad. Propuesto contrato de interfaz v1 (MQTT recomendado, pendiente de acordar) |
