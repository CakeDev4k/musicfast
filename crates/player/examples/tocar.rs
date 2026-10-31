//! Prova de ponta a ponta do motor, sem interface e em volume zero (o
//! callback roda e decodifica, mas nada sai no alto-falante de quem testa):
//!
//!     cargo run --release -p musicfast-player --example tocar -- "busca" <pasta-rascunho>
//!
//! Mede o tempo até a posição andar (resolver + baixar + decodificar), pula
//! perto do fim para ver a próxima entrar sozinha, e o "voltar".

use std::sync::Arc;
use std::time::{Duration, Instant};

use musicfast_player::{Command, Config, Player, Status};
use musicfast_ytm::{Client, SearchFilter};

fn wait(player: &Player, what: &str, timeout: Duration, ok: impl Fn(&Player) -> bool) -> Duration {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if ok(player) {
            let took = started.elapsed();
            println!("{what}: {} ms", took.as_millis());
            return took;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let s = player.snapshot();
    panic!("{what}: não aconteceu em {timeout:?} (status {:?}, passo {:?}, aviso {:?})", s.status, s.step, s.notice);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let query = args.next().unwrap_or_else(|| "rick astley never gonna give you up".into());
    let scratch = std::path::PathBuf::from(args.next().expect("pasta de rascunho"));
    let client = Arc::new(Client::new("pt", "BR").unwrap());
    let t = Instant::now();
    let page = client.search(&query, SearchFilter::Songs).unwrap();
    let seed = page.tracks().next().expect("nenhuma música").clone();
    println!("busca: {} ms -> {} ({})", t.elapsed().as_millis(), seed.title, seed.artist_names());

    let mut player = Player::start(
        Config {
            bin_dir: scratch.join("bin"),
            cache_file: scratch.join("enderecos.json"),
            client,
            volume: 0.0,
            offline: None,
        },
        Arc::new(|| {}),
    );
    player.send(Command::PlayRadio { seed });
    wait(&player, "primeiro som (posição > 0,3 s)", Duration::from_secs(120), |p| {
        p.snapshot().status == Status::Playing && p.position() > 0.3
    });
    wait(&player, "fila do rádio", Duration::from_secs(10), |p| p.snapshot().queue.len() > 5);
    let s = player.snapshot();
    println!("fila: {} músicas; duração da atual {:?}", s.queue.len(), s.duration);

    // A próxima é resolvida e baixada enquanto a atual toca.
    std::thread::sleep(Duration::from_secs(8));
    let duration = player.snapshot().duration.unwrap_or(200.0);
    player.send(Command::Seek(duration - 2.0));
    let index = player.snapshot().index;
    wait(&player, "troca automática para a próxima", Duration::from_secs(30), |p| {
        let s = p.snapshot();
        s.index != index && s.status == Status::Playing && p.position() < 5.0 && p.position() > 0.05
    });
    player.send(Command::Next);
    wait(&player, "Próxima (sem pré-carga)", Duration::from_secs(60), |p| {
        p.snapshot().status == Status::Playing && p.position() > 0.1
    });
    player.send(Command::Previous);
    wait(&player, "Voltar (bytes guardados)", Duration::from_secs(30), |p| {
        p.snapshot().status == Status::Playing && p.position() > 0.1
    });
    player.send(Command::Pause);
    wait(&player, "pausar", Duration::from_secs(2), |p| p.snapshot().status == Status::Paused);
    player.shutdown();
    let cache = std::fs::read_to_string(scratch.join("enderecos.json")).unwrap_or_default();
    println!("cache de endereços: {} bytes", cache.len());
}
