//! UI minimale Slint per wasmbox-core.
//!
//! Due modalità:
//!
//! - **Headless screenshot**:
//!   `cargo run -p wasmbox-ui -- --screenshot out.bmp [guest.wasm] [input]` —
//!   renderizza l'interfaccia con lo `SoftwareRenderer` su un buffer in
//!   memoria, esegue una run reale del guest e salva il frame in BMP.
//!   Exit code: 0 run guest + screenshot ok; 2 errore d'uso;
//!   11 se il render è vuoto; 12 se la sandbox ha fallito.
//! - **Interattiva** (serve un backend di sistema, es. winit): il Platform
//!   custom qui gestisce solo il rendering software; senza backend grafico
//!   l'esecuzione interattiva non è disponibile e si usa --screenshot.

use std::path::PathBuf;
use std::rc::Rc;

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, SoftwareRenderer, TargetPixel,
};
use slint::platform::{Platform, PlatformError};

use wasmbox_core::{HostError, HostHandler, SandboxConfig, SandboxEngine, SandboxError};

slint::include_modules!();

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

/// Pixel RGB puro: 3 byte, usato come TargetPixel del renderer software.
#[derive(Clone, Copy, PartialEq)]
struct RgbPixel([u8; 3]);

impl TargetPixel for RgbPixel {
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        let a = color.alpha as u32;
        for (dst, src) in self.0.iter_mut().zip([color.red, color.green, color.blue]) {
            *dst = ((src as u32 * a + (*dst as u32) * (255 - a)) / 255) as u8;
        }
    }
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        RgbPixel([r, g, b])
    }
}

/// Handler della demo: eco, come CLI/host-run.
struct EchoHandler;
impl HostHandler for EchoHandler {
    fn ask(&mut self, req: &[u8]) -> Result<Vec<u8>, HostError> {
        Ok(String::from_utf8_lossy(req).to_uppercase().into_bytes())
    }
}

/// Platform custom: espone SOLO la finestra minimal con il software renderer.
/// La `MinimalSoftwareWindow` la teniamo nel chiamante per `draw_if_needed`
/// (metodo che non c'è sull'handle generico `Window`).
struct HeadlessPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        // Il processo parte a un istante arbitrario; per la UI demo basta
        // un valore monotono nonuscito da un wrapper availabile.
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed()
    }
}

/// Esegue il guest nella sandbox.
fn run_guest(wasm_path: &PathBuf, input: &[u8]) -> Result<Vec<u8>, SandboxError> {
    let wasm = std::fs::read(wasm_path)
        .map_err(|e| SandboxError::EngineInit(format!("lettura guest fallita: {e}")))?;
    let config = SandboxConfig {
        cache_dir: Some(std::env::temp_dir().join("wasmbox-ui-cache")),
        ..Default::default()
    };
    let engine = SandboxEngine::new(&wasm, config)?;
    let mut handler = EchoHandler;
    let result = engine.run(input, &mut handler);
    // L'output del guest lo mostriamo su stderr: utile per l'uso CLI, non
    // sporca stdout (che rimane libero in vista futura --json).
    match &result {
        Ok(out) => {
            let text = String::from_utf8_lossy(out);
            eprintln!("[ui] guest ok → {text}");
        }
        Err(e) => eprintln!("[ui] guest errore: {e}"),
    }
    result
}

/// Uso: `wasmbox-ui [--screenshot out.bmp] [guest.wasm] [input]`
///
/// I flag (`--screenshot <path>`) si possono mettere in qualsiasi posizione;
/// il primo argomento posizionale è il guest .wasm, il secondo è l'input.
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut wasm_path: Option<PathBuf> = None;
    let mut input: Option<String> = None;
    let mut screenshot_out: Option<PathBuf> = None;

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--screenshot" => match it.next() {
                Some(path) => screenshot_out = Some(PathBuf::from(path)),
                None => {
                    eprintln!("errore: --screenshot richiede un percorso BMP");
                    return std::process::ExitCode::from(2);
                }
            },
            "--help" | "-h" => {
                eprintln!("uso: wasmbox-ui [--screenshot out.bmp] [guest.wasm] [input]");
                return std::process::ExitCode::from(0);
            }
            _ if wasm_path.is_none() => wasm_path = Some(PathBuf::from(arg)),
            _ if input.is_none() => input = Some(arg),
            _ => {
                eprintln!("errore: argomento inatteso: {arg}");
                return std::process::ExitCode::from(2);
            }
        }
    }

    let input = input.unwrap_or_else(|| "ciao da slint ui".into());
    let wasm_path =
        wasm_path.unwrap_or_else(|| "target/wasm32-unknown-unknown/release/guest_echo.wasm".into());

    let headless_window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::NewBuffer,
    );
    slint::platform::set_platform(Box::new(HeadlessPlatform {
        window: headless_window.clone(),
    }))
    .expect("set_platform headless");

    let app = SandboxWindow::new().expect("creazione UI");

    // Run reale del guest PRIMA del rendering: il colore del quadrato riflette
    // l'esito vero (verde ok / rosso fallito / grigio senza run).
    let result = run_guest(&wasm_path, input.as_bytes());
    match &result {
        Ok(_) => {
            app.set_has_run(true);
            app.set_last_ok(true);
        }
        Err(_) => {
            app.set_has_run(true);
            app.set_last_ok(false);
        }
    }

    // show() fa il layout e assegna alla MinimalSoftwareWindow la dimensione
    // del root; senza, la finestra headless resta a 0x0 e il render è vuoto.
    app.show()
        .expect("show (headless: solo dimensione + layout)");
    headless_window.set_size(slint::PhysicalSize::new(WIDTH, HEIGHT));

    if let Some(path) = screenshot_out {
        // Modalità headless: redraw + render su buffer + BMP su disco.
        headless_window.request_redraw();

        let mut buffer = vec![RgbPixel([255, 255, 255]); (WIDTH * HEIGHT) as usize];
        let mut rendered = false;
        headless_window.draw_if_needed(|renderer: &SoftwareRenderer| {
            renderer.render(&mut buffer, WIDTH as usize);
            rendered = true;
        });

        if !rendered {
            eprintln!("[ui] ERRORE: il renderer non ha disegnato nulla.");
            return std::process::ExitCode::from(11);
        }
        write_bmp(&path, &buffer, WIDTH, HEIGHT);
        eprintln!("[ui] screenshot: {} ({}x{})", path.display(), WIDTH, HEIGHT);
        return match &result {
            Ok(_) => std::process::ExitCode::from(0),
            Err(e) => {
                eprintln!("[ui] run guest fallita: {e}");
                std::process::ExitCode::from(12)
            }
        };
    }

    // Modalità interattiva: richiede un backend di sistema (winit non incluso
    // nelle features di default); seno backend grafico usa la modalità
    // --screenshot descritta sopra.
    app.show().expect("show");
    slint::run_event_loop_until_quit().expect("event loop");
    std::process::ExitCode::from(0)
}

/// Scrive un BMP 24-bit non compresso. Nessuna dipendenza: 54 byte di header
/// + righe bottom-up, padded a 4 byte.
fn write_bmp(path: &std::path::Path, buf: &[RgbPixel], w: u32, h: u32) {
    const HEADER_SIZE: u32 = 54;
    let row_size = (3 * w).div_ceil(4) * 4; // padding a 4 byte
    let pixel_bytes = row_size * h;
    let file_size = HEADER_SIZE + pixel_bytes;

    let mut out = Vec::with_capacity(file_size as usize);
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]); // reserved
    out.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    // BITMAPINFOHEADER (40 byte)
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&24u16.to_le_bytes()); // bpp
    out.extend_from_slice(&0u32.to_le_bytes()); // compression = BI_RGB
    out.extend_from_slice(&pixel_bytes.to_le_bytes());
    out.extend_from_slice(&2835u32.to_le_bytes()); // ppm horiz
    out.extend_from_slice(&2835u32.to_le_bytes()); // ppm vert
    out.extend_from_slice(&0u32.to_le_bytes()); // palette colors
    out.extend_from_slice(&0u32.to_le_bytes()); // important colors

    for row in 0..h {
        let y = h - 1 - row; // BMP bottom-up
        for x in 0..w {
            let p = &buf[(y * w + x) as usize].0;
            out.extend_from_slice(&[p[2], p[1], p[0]]); // BMP = BGR
        }
        out.extend(std::iter::repeat_n(0, (row_size - 3 * w) as usize));
    }
    std::fs::write(path, &out).expect("scrittura BMP");
}
