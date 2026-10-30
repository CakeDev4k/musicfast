//! O leitor contra respostas reais do YouTube Music, gravadas em 28/10/2026
//! (hl=pt, gl=BR, sem login). Quando o YouTube mudar o formato, regrave com
//! o mesmo pedido e veja o que reprova.

use musicfast_ytm::parse;
use musicfast_ytm::{Item, Kind, ShelfStyle, Thumb, Thumbs, Track};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn tracks(page: &musicfast_ytm::Page) -> Vec<&Track> {
    page.tracks().collect()
}

#[test]
fn song_search_reads_artists_album_and_duration() {
    let page = parse::page(&fixture("search_songs"));
    let t = tracks(&page);
    assert_eq!(t.len(), 20);
    let first = t[0];
    assert_eq!(first.video_id, "MkBAhMaqYNY");
    assert_eq!(first.title, "Downtown");
    let names: Vec<&str> = first.artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Anitta", "J Balvin"]);
    assert_eq!(first.artists[0].id.as_deref(), Some("UCI99G32byWj1aH13pbxnTEw"));
    assert_eq!(first.album.as_ref().unwrap().id.as_deref(), Some("MPREb_90Drj3qQIKp"));
    assert_eq!(first.duration_secs, Some(194));
    assert!(!first.thumbs.is_empty());
    // A contagem de plays da terceira coluna não vira artista nem álbum.
    assert!(t.iter().all(|t| t.artists.iter().all(|a| !a.name.contains("Tocou"))));
    assert!(t.iter().all(|t| t.duration_secs.is_some()), "toda música da busca tem duração");
}

#[test]
fn search_all_groups_the_flat_list_by_kind() {
    let page = parse::page(&fixture("search_all"));
    let titles: Vec<&str> = page.shelves.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles[0], "Melhor resultado", "{titles:?}");
    let Item::Card(top) = &page.shelves[0].items[0] else { panic!("o melhor resultado é o artista") };
    assert_eq!(top.kind, Kind::Artist);
    assert_eq!(top.title, "Anitta");
    for expected in ["Artistas", "Músicas", "Álbuns", "Playlists", "Vídeos"] {
        assert!(titles.contains(&expected), "falta {expected}: {titles:?}");
    }
    let songs = page.shelves.iter().find(|s| s.title == "Músicas").unwrap();
    let Item::Track(show) = &songs.items[0] else { panic!() };
    assert_eq!(show.title, "Show das Poderosas");
    assert_eq!(show.artist_names(), "Anitta");
    let artists = page.shelves.iter().find(|s| s.title == "Artistas").unwrap();
    assert_eq!(artists.style, ShelfStyle::Carousel);
}

#[test]
fn home_has_carousels_and_more() {
    let page = parse::page(&fixture("home"));
    assert!(page.shelves.len() >= 2);
    for shelf in &page.shelves {
        assert!(!shelf.title.is_empty());
        assert!(!shelf.items.is_empty(), "{}", shelf.title);
    }
    let Item::Card(card) = &page.shelves[0].items[0] else { panic!("o início abre com capas") };
    assert_eq!(card.kind, Kind::Playlist);
    assert!(card.play.as_ref().and_then(|p| p.playlist_id.as_ref()).is_some());
    assert!(page.continuation.is_some());
}

#[test]
fn radio_queue_starts_at_the_song_and_points_to_lyrics() {
    let watch = parse::watch(&fixture("next"));
    assert_eq!(watch.tracks.len(), 50);
    assert_eq!(watch.tracks[0].video_id, "lYBUbBu4W08");
    let second = &watch.tracks[1];
    assert_eq!(second.title, "Whenever You Need Somebody");
    assert_eq!(second.artist_names(), "Rick Astley");
    assert_eq!(second.duration_secs, Some(234));
    assert_eq!(second.album.as_ref().unwrap().name, "Whenever You Need Somebody");
    assert!(watch.lyrics_id.as_deref().unwrap().starts_with("MPLY"));
    assert!(watch.related_id.is_some());
    assert!(watch.continuation.is_some());

    let more = parse::watch_continuation(&fixture("next_cont"));
    assert!(more.tracks.len() >= 10, "{}", more.tracks.len());
    assert!(more.continuation.is_some());
}

#[test]
fn album_tracks_take_artist_and_cover_from_the_header() {
    let mut page = parse::page(&fixture("album"));
    let header = page.header.take().unwrap();
    assert_eq!(header.title, "Downtown");
    assert_eq!(header.authors[0].name, "Anitta");
    assert!(header.play.as_ref().unwrap().playlist_id.as_deref().unwrap().starts_with("OLAK"));
    let page = parse::page(&fixture("album"));
    let t = tracks(&page);
    assert_eq!(t[0].video_id, "wlS6Ix7mA0w");
    assert_eq!(t[0].duration_secs, Some(194));
    assert!(!t[0].artists.is_empty());
    assert!(!t[0].thumbs.is_empty());
}

#[test]
fn artist_page_has_header_songs_and_carousels() {
    let page = parse::page(&fixture("artist"));
    let header = page.header.as_ref().unwrap();
    assert_eq!(header.kind, Kind::Artist);
    assert_eq!(header.title, "Anitta");
    assert!(!header.thumbs.is_empty());
    assert!(page.shelves[0].style == ShelfStyle::List && page.shelves[0].items.len() == 5);
    assert!(page.shelves.iter().filter(|s| s.style == ShelfStyle::Carousel).count() >= 5);
}

#[test]
fn playlist_reads_every_track_with_album() {
    let page = parse::page(&fixture("playlist"));
    let t = tracks(&page);
    assert_eq!(t.len(), 62);
    assert!(t.iter().all(|t| t.duration_secs.is_some()));
    // Nesta playlist editorial só 3 das 62 faixas trazem a coluna do álbum
    // (conferido no JSON): elas têm álbum, e nenhum é inventado nas outras.
    let with_album: Vec<&str> = t.iter().filter_map(|t| t.album.as_ref()).map(|a| a.name.as_str()).collect();
    assert_eq!(with_album.len(), 3, "{with_album:?}");
    assert_eq!(t[35].album.as_ref().unwrap().id.as_deref(), Some("MPREb_8BZ6ibk6ap9"));
    assert!(t.iter().all(|t| !t.artists.is_empty()));
    assert_eq!(t[0].title, "Take On Me");
    assert_eq!(t[0].artist_names(), "a-ha");
    assert!(page.header.is_some());
}

#[test]
fn lyrics_and_suggestions() {
    let lyrics = parse::lyrics(&fixture("lyrics")).unwrap();
    assert!(lyrics.text.contains("Never gonna give you up"));
    let s = parse::suggestions(&fixture("suggest"));
    assert!(!s.is_empty());
    assert!(s.iter().all(|s| s.to_lowercase().contains("anit")), "{s:?}");
}

#[test]
fn cover_urls_carry_the_size_and_video_thumbs_pick_the_smallest_that_fits() {
    let cover = Thumbs(vec![
        Thumb { url: "https://yt3.googleusercontent.com/abc=w60-h60-l90-rj".into(), width: 60 },
        Thumb { url: "https://yt3.googleusercontent.com/abc=w120-h120-l90-rj".into(), width: 120 },
    ]);
    assert_eq!(cover.url_for(96).unwrap(), "https://yt3.googleusercontent.com/abc=w96-h96-l90-rj");
    let video = Thumbs(vec![
        Thumb { url: "https://i.ytimg.com/vi/x/default.jpg".into(), width: 120 },
        Thumb { url: "https://i.ytimg.com/vi/x/hqdefault.jpg".into(), width: 480 },
    ]);
    assert_eq!(video.url_for(200).unwrap(), "https://i.ytimg.com/vi/x/hqdefault.jpg");
    assert_eq!(parse::duration("1:02:03"), Some(3723));
    assert_eq!(parse::duration("2026"), None);
}

/// Contra o YouTube de verdade: `cargo test -p musicfast-ytm -- --ignored`.
#[test]
#[ignore]
fn live_search_and_radio() {
    let c = musicfast_ytm::Client::new("pt", "BR").unwrap();
    let page = c.search("anitta envolver", musicfast_ytm::SearchFilter::Songs).unwrap();
    let first = page.tracks().next().unwrap();
    let watch = c.watch(Some(&first.video_id), None, None).unwrap();
    assert!(watch.tracks.len() > 10);
    assert!(c.home().unwrap().shelves.len() >= 2);
}

#[test]
fn logged_out_responses_say_so() {
    let v = fixture("home");
    assert_eq!(parse::logged_in(&v), Some(false));
    assert_eq!(parse::session(&v), musicfast_ytm::Session::SignedOut);
    // Uma resposta que o leitor não reconhece não é sessão vencida.
    assert_eq!(parse::session(&serde_json::json!({})), musicfast_ytm::Session::Unknown);
}
