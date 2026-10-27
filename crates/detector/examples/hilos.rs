use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::{Duration, Instant};

fn main() {
    // Canal ACOTADO: caben como maximo 3 bloques esperando
    let (tx, rx) = sync_channel::<u32>(3);

    // Hilo productor: simula el microfono *un bloque cada 100 ms)
    let captura = thread::spawn(move || {
        let inicio = Instant::now();
        for n in 1..=10 {
            thread::sleep(Duration::from_millis(100));
            // `send` BLOQUEA: si el canal está lleno, se queda esperando aquí.
            if tx.send(n).is_err() {
                break;
            }
            println!(
                "captura: bloque {n} enviado a los {} ms",
                inicio.elapsed().as_millis()
            );
        }
    });

    //Hilo principal: simula el analisis (tarda 250 ms por bloque)
    for bloque in rx {
        println!("Analisis: Procesando bloque {bloque}...");
        thread::sleep(Duration::from_millis(250));
    }

    if captura.join().is_err() {
        eprintln!("El hilo de captura entro en panico");
    }
    println!("Fin");
}
