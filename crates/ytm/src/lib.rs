//! O catálogo do YouTube Music.
//!
//! Não há API oficial. O site fala com `music.youtube.com/youtubei/v1/*`
//! (InnerTube) com o cliente `WEB_REMIX`, e é isso que este crate faz: sem
//! chave, sem login, cada chamada medida em 80–380 ms (28/10/2026).
//!
//! O tocar não mora aqui: o endereço do áudio exige o desafio de JavaScript
//! do player, que o yt-dlp resolve (`musicfast-player`).
//!
//! **Um leitor só.** As respostas são árvores de "renderers"; em vez de um
//! leitor por página, `parse` converte qualquer uma em `Page` (cabeçalho +
//! prateleiras de `Item`). Quando o YouTube trocar um nome, quebra num lugar,
//! e os testes com respostas reais gravadas (`tests/fixtures`) acusam.

pub mod client;
mod json;
pub mod model;
pub mod parse;

pub use client::{Auth, Client, SearchFilter};
pub use model::*;
