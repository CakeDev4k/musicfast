//! Os textos de um idioma. Os com pedaços variáveis são modelos (`{}`,
//! `{v}`, `{r}`) lidos pelos métodos daqui, para os idiomas poderem mudar a
//! ordem das palavras.

pub struct Strings {
    // A coluna da esquerda e a barra de cima.
    pub home: &'static str,
    pub explore: &'static str,
    pub library: &'static str,
    pub expand_menu: &'static str,
    pub collapse_menu: &'static str,
    pub new_playlist: &'static str,
    pub sign_in_invite: &'static str,
    pub sign_in: &'static str,
    pub signing_in: &'static str,
    pub sign_in_with_cookies: &'static str,
    pub sign_in_pasting_cookies: &'static str,
    pub liked_songs: &'static str,
    pub auto_playlist: &'static str,
    pub your_library: &'static str,
    pub sign_out: &'static str,
    pub manage_google_account: &'static str,
    pub back_tip: &'static str,
    pub forward_tip: &'static str,
    pub search_hint: &'static str,
    pub language: &'static str,
    pub(crate) language_auto: &'static str,

    // As abas da biblioteca e os filtros da busca.
    pub all: &'static str,
    pub playlists: &'static str,
    pub songs: &'static str,
    pub videos: &'static str,
    pub albums: &'static str,
    pub artists: &'static str,
    pub subscriptions: &'static str,
    pub history: &'static str,
    pub episodes: &'static str,
    pub podcasts: &'static str,
    pub others: &'static str,
    pub top_result: &'static str,

    // Privacidade da playlist.
    pub private: &'static str,
    pub unlisted: &'static str,
    pub public: &'static str,

    // Os diálogos.
    pub title_hint: &'static str,
    pub privacy: &'static str,
    pub create: &'static str,
    pub cancel: &'static str,
    pub close: &'static str,
    pub save_to_playlist: &'static str,
    pub no_playlists_yet: &'static str,
    pub cookie_steps: [&'static str; 4],
    pub enter: &'static str,
    pub cookies_without_session: &'static str,

    // As páginas.
    pub load_failed: &'static str,
    pub try_again: &'static str,
    pub nothing_found: &'static str,
    pub play: &'static str,
    pub shuffle: &'static str,
    pub radio: &'static str,
    pub subscribed: &'static str,
    pub subscribe: &'static str,
    pub in_library: &'static str,
    pub save_to_library: &'static str,
    pub play_all: &'static str,
    pub more: &'static str,
    pub remove_from_liked: &'static str,
    pub like: &'static str,
    pub play_next: &'static str,
    pub add_to_queue: &'static str,
    pub start_radio: &'static str,
    pub go_to_album: &'static str,
    pub go_to_artist: &'static str,
    pub sign_in_prompt_title: &'static str,
    pub sign_in_prompt_body: &'static str,

    // A barra do player e o mini player.
    pub previous: &'static str,
    pub next: &'static str,
    pub pause: &'static str,
    pub play_tip: &'static str,
    pub pause_tip: &'static str,
    pub now_playing: &'static str,
    pub pin_mini: &'static str,
    pub pin_mini_tip: &'static str,
    pub liked: &'static str,
    pub disliked: &'static str,
    pub show_queue: &'static str,
    pub close_queue: &'static str,
    pub show_lyrics: &'static str,
    pub close_lyrics: &'static str,
    pub open_player: &'static str,
    pub close_player: &'static str,
    pub shuffle_on: &'static str,
    pub repeat_queue: &'static str,
    pub repeat_one: &'static str,
    pub no_repeat: &'static str,
    pub(crate) volume: &'static str,
    pub back_to_window: &'static str,
    pub nothing_playing: &'static str,

    // A tela "Tocando agora", a fila e a letra.
    pub up_next: &'static str,
    pub lyrics: &'static str,
    pub related: &'static str,
    pub queue: &'static str,
    pub(crate) queue_one: &'static str,
    pub(crate) queue_many: &'static str,
    pub playing_on_this_device: &'static str,
    pub nothing_related: &'static str,
    pub queue_empty: &'static str,
    pub remove_from_queue: &'static str,
    pub play_to_see_lyrics: &'static str,
    pub no_lyrics: &'static str,
    pub lyrics_not_synced: &'static str,
    pub sync: &'static str,
    pub instrumental: &'static str,
    pub lyrics_by_lrclib: &'static str,
    pub lyrics_by_netease: &'static str,
    pub lyrics_stretched: &'static str,
    pub lyrics_other: &'static str,
    pub lyrics_translation: &'static str,
    pub lyrics_translation_show: &'static str,
    pub lyrics_translation_hide: &'static str,
    pub look_section_lyrics: &'static str,
    pub lyrics_style_hint: &'static str,
    pub lyrics_font: &'static str,
    pub font_default: &'static str,
    pub font_serif: &'static str,
    pub font_modern: &'static str,
    pub font_fun: &'static str,
    pub font_script: &'static str,
    pub font_mono: &'static str,
    pub lyrics_color: &'static str,
    pub lyrics_size: &'static str,
    pub lyrics_blur: &'static str,
    pub lyrics_dim: &'static str,
    pub lyrics_align: &'static str,
    pub align_left: &'static str,
    pub align_center: &'static str,
    pub align_right: &'static str,
    pub translation_same_font: &'static str,
    pub font_search: &'static str,
    pub fonts_loading: &'static str,
    pub(crate) fonts_count: &'static str,
    pub lyrics_text_group: &'static str,
    pub lyrics_effects_group: &'static str,
    pub translation_font: &'static str,
    pub lyrics_romaji: &'static str,
    pub lyrics_romaji_hint: &'static str,
    pub lyrics_hover_buttons: &'static str,
    pub lyrics_hover_buttons_hint: &'static str,
    pub translation_hint: &'static str,
    pub translation_shape: &'static str,
    pub translation_below: &'static str,
    pub translation_above: &'static str,
    pub translation_replace: &'static str,
    pub translation_language: &'static str,
    pub translation_app_language: &'static str,
    pub translation_method: &'static str,
    pub method_google: &'static str,
    pub method_mymemory: &'static str,
    pub method_google_hint: &'static str,
    pub method_mymemory_hint: &'static str,
    pub method_local: &'static str,
    pub method_local_hint: &'static str,
    pub local_not_found: &'static str,
    pub local_no_model: &'static str,
    pub local_checking: &'static str,
    pub local_none: &'static str,
    pub local_get_ollama: &'static str,
    pub local_ollama_empty: &'static str,
    pub local_download: &'static str,
    pub local_downloading: &'static str,
    pub local_download_failed: &'static str,
    pub local_model: &'static str,
    pub local_auto: &'static str,
    pub local_using_ollama: &'static str,
    pub local_using_libre: &'static str,
    pub local_retry: &'static str,
    pub translation_color: &'static str,
    pub lyrics_style_button: &'static str,
    pub lyrics_sample_translation: &'static str,
    pub lyrics_searching: &'static str,
    pub lyrics_no_other: &'static str,
    pub(crate) lyrics_choice: &'static str,
    pub(crate) lrclib_busy: &'static str,
    pub lyrics_internal_error: &'static str,

    // Os avisos.
    pub nothing_to_play: &'static str,
    pub sign_in_in_window: &'static str,
    pub signed_out: &'static str,
    /// O item do menu e o título do diálogo do cache.
    pub storage: &'static str,
    pub storage_hint: &'static str,
    pub storage_lyrics: &'static str,
    pub storage_covers: &'static str,
    pub storage_browser: &'static str,
    pub storage_total: &'static str,
    pub storage_measuring: &'static str,
    pub clear_cache: &'static str,
    pub clear_cache_keeps: &'static str,
    pub(crate) cache_cleared: &'static str,
    pub update_install_failed: &'static str,
    pub added_to_liked: &'static str,
    pub thanks_for_feedback: &'static str,
    pub removed_from_liked: &'static str,
    pub saved_to_library: &'static str,
    pub removed_from_library: &'static str,
    pub subscription_done: &'static str,
    pub subscription_cancelled: &'static str,
    pub(crate) saved_in: &'static str,
    pub(crate) playlist_created: &'static str,
    pub session_missing: &'static str,
    pub connected: &'static str,
    pub load_more_failed: &'static str,
    pub session_expired: &'static str,
    pub login_cancelled: &'static str,
    pub login_window_failed: &'static str,
    pub(crate) action_failed: &'static str,
    pub(crate) updated_to: &'static str,
    pub update_rolled_back: &'static str,

    // A atualização.
    pub(crate) downloading_percent: &'static str,
    pub(crate) downloading_version: &'static str,
    pub restart_to_update: &'static str,
    pub(crate) version_ready: &'static str,
    pub new_version: &'static str,
    pub(crate) version_out: &'static str,
    pub auto_download_failed: &'static str,
    pub move_to_applications: &'static str,
    pub update_with_homebrew: &'static str,
    pub update_with_package_manager: &'static str,
    pub not_from_releases: &'static str,
    pub download_from_releases: &'static str,

    // A personalização do player.
    pub customize_player: &'static str,
    pub bar_buttons: &'static str,
    pub bar_hint: &'static str,
    pub bar_restore: &'static str,
    pub bar_zone_left: &'static str,
    pub bar_zone_track: &'static str,
    pub bar_zone_center: &'static str,
    pub bar_zone_right: &'static str,
    pub bar_catalog: &'static str,
    pub bar_catalog_hint: &'static str,
    pub bar_drop_remove: &'static str,
    pub bar_faded: &'static str,
    pub bar_drop_here: &'static str,
    pub item_play_pause: &'static str,
    pub item_repeat: &'static str,
    pub item_time: &'static str,
    pub item_menu: &'static str,
    pub item_volume: &'static str,
    pub item_queue: &'static str,
    pub item_lyrics: &'static str,
    pub item_mini: &'static str,
    pub item_expand: &'static str,
    pub item_download: &'static str,
    pub item_save: &'static str,
    pub item_album: &'static str,
    pub item_artist: &'static str,
    pub item_personalize: &'static str,
    pub copy_link: &'static str,
    pub link_copied: &'static str,
    pub item_seek_back: &'static str,
    pub item_seek_forward: &'static str,
    pub discord_show: &'static str,
    pub discord_showing: &'static str,
    pub discord_waiting: &'static str,
    pub discord_refused: &'static str,
    pub discord_off: &'static str,
    pub discord_promote: &'static str,
    pub discord_promote_hint: &'static str,
    pub discord_site_button: &'static str,
    pub discord_listen: &'static str,
    pub discord_turned_on: &'static str,
    pub discord_turned_off: &'static str,
    pub player_background: &'static str,
    pub look_section_background: &'static str,
    pub look_dim: &'static str,
    pub look_blur: &'static str,
    pub look_default: &'static str,
    pub look_cover: &'static str,
    pub look_color: &'static str,
    pub look_image: &'static str,
    pub look_color_label: &'static str,
    pub image_url_hint: &'static str,
    pub image_formats: &'static str,
    pub apply: &'static str,
    pub loading_image: &'static str,
    pub(crate) image_load_failed: &'static str,
    pub look_hint_plain: &'static str,
    pub look_hint_cover: &'static str,
    pub look_hint_color: &'static str,
    pub custom_color: &'static str,
    pub image_applied: &'static str,
    pub viz_title: &'static str,
    pub viz_hint: &'static str,
    pub viz_off: &'static str,
    pub viz_bars: &'static str,
    pub viz_mirror: &'static str,
    pub viz_line: &'static str,
    pub viz_circle: &'static str,
    pub viz_mountain: &'static str,
    pub viz_led: &'static str,
    pub viz_dots: &'static str,
    pub viz_curve: &'static str,
    pub viz_silk: &'static str,
    pub viz_pulse: &'static str,
    pub viz_aurora: &'static str,
    pub viz_color_cover: &'static str,
    pub viz_color_rainbow: &'static str,
    pub viz_color_gradient: &'static str,
    pub viz_color_solid: &'static str,
    pub viz_settings: &'static str,
    pub viz_reset: &'static str,
    pub viz_colors: &'static str,
    pub viz_from: &'static str,
    pub viz_to: &'static str,
    pub viz_swap: &'static str,
    pub viz_count: &'static str,
    pub viz_sensitivity: &'static str,
    pub viz_opacity: &'static str,
    pub viz_thickness: &'static str,
    pub viz_glow: &'static str,
    pub viz_glow_hint: &'static str,
    pub surface_player: &'static str,
    pub surface_bar: &'static str,
    pub surface_mini: &'static str,
    pub surface_home: &'static str,
    pub look_section_mini: &'static str,
    pub mini_drag_hint: &'static str,
    pub mini_size: &'static str,
    pub mini_lyrics_divider: &'static str,
    pub mini_lyrics_divider_hint: &'static str,
    pub mini_buttons_group: &'static str,
    pub pick_from_computer: &'static str,
    pub drop_hint: &'static str,
    pub or_paste_link: &'static str,
    pub image_files: &'static str,

    // O login.
    pub login_window_title: &'static str,
    pub login_window_crashed: &'static str,
    pub(crate) login_spawn_failed: &'static str,

    // Os erros do catálogo.
    pub ytm_internal_error: &'static str,
    pub(crate) ytm_no_connection: &'static str,
    pub(crate) ytm_status: &'static str,
    pub(crate) ytm_unexpected: &'static str,
    pub(crate) playlist_answered: &'static str,

    // O player.
    pub building_queue: &'static str,
    pub downloading: &'static str,
    pub preparing_ytdlp: &'static str,
    pub fetching_audio: &'static str,
    pub finding_similar: &'static str,
    pub queue_not_built: &'static str,
    pub(crate) queue_error: &'static str,
    pub(crate) no_audio_output: &'static str,
    pub(crate) ytdlp_install_failed: &'static str,
    pub ytdlp_already_latest: &'static str,
    pub audio_url_refused: &'static str,
    pub(crate) audio_server_answered: &'static str,
    pub(crate) download_failed: &'static str,
    pub(crate) download_interrupted: &'static str,
    pub audio_too_long: &'static str,
    pub audio_empty: &'static str,
    pub(crate) audio_format_unknown: &'static str,
    pub audio_no_track: &'static str,
    pub audio_no_params: &'static str,
    pub audio_no_rate: &'static str,
    pub(crate) audio_no_decoder: &'static str,
    pub audio_corrupted: &'static str,
    pub audio_decoder_failed: &'static str,

    // As músicas baixadas (para ouvir sem internet e arrastar para fora).
    pub downloads: &'static str,
    pub download: &'static str,
    pub delete_download: &'static str,
    pub show_in_folder: &'static str,
    pub open_folder: &'static str,
    pub download_queued: &'static str,
    pub download_preparing: &'static str,
    pub download_saving: &'static str,
    pub drag_out_hint: &'static str,
    pub change_folder: &'static str,
    pub change_folder_hint: &'static str,
    pub default_folder: &'static str,
    pub(crate) saving_to: &'static str,
    pub(crate) folder_unusable: &'static str,
    pub no_downloads: &'static str,
    pub(crate) download_failed_retry: &'static str,
    pub(crate) downloads_one: &'static str,
    pub(crate) downloads_many: &'static str,
    pub interface: &'static str,
    pub interface_classic: &'static str,
    pub interface_panels: &'static str,
    pub show_all: &'static str,
    pub album: &'static str,
    pub filter_library: &'static str,
    pub add_account: &'static str,
    pub(crate) switch_to: &'static str,
    pub(crate) now_in_account: &'static str,
    pub filter_downloads: &'static str,
    pub no_album: &'static str,
    pub unknown_artist: &'static str,
    pub various_artists: &'static str,
    pub sort_recent: &'static str,
    pub sort_title: &'static str,
    pub sort_artist: &'static str,
    pub(crate) songs_one: &'static str,
    pub(crate) songs_many: &'static str,
    pub(crate) albums_one: &'static str,
    pub(crate) albums_many: &'static str,
    pub(crate) delete_download_failed: &'static str,
    pub(crate) drag_out_failed: &'static str,
}

fn fill(template: &str, value: impl std::fmt::Display) -> String {
    template.replace("{}", &value.to_string())
}

impl Strings {
    /// "Automático (Português)".
    pub fn language_auto(&self, detected: &str) -> String {
        fill(self.language_auto, detected)
    }

    pub fn volume(&self, percent: f32) -> String {
        fill(self.volume, percent.round())
    }

    pub fn queue_count(&self, n: usize) -> String {
        fill(if n == 1 { self.queue_one } else { self.queue_many }, n)
    }

    pub fn lrclib_busy(&self, code: u16) -> String {
        fill(self.lrclib_busy, code)
    }

    pub fn saved_in(&self, playlist: &str) -> String {
        fill(self.saved_in, playlist)
    }

    pub fn playlist_created(&self, title: &str) -> String {
        fill(self.playlist_created, title)
    }

    pub fn action_failed(&self, error: &str) -> String {
        fill(self.action_failed, error)
    }

    pub fn updated_to(&self, version: &str) -> String {
        fill(self.updated_to, version)
    }

    /// `progress` de 0 a 1.
    pub fn downloading_percent(&self, progress: f32) -> String {
        fill(self.downloading_percent, format!("{:.0}", progress * 100.0))
    }

    pub fn downloading_version(&self, version: &str) -> String {
        fill(self.downloading_version, version)
    }

    pub fn version_ready(&self, version: &str) -> String {
        fill(self.version_ready, version)
    }

    pub fn version_out(&self, version: &str, reason: &str) -> String {
        self.version_out.replace("{v}", version).replace("{r}", reason)
    }

    pub fn image_load_failed(&self, error: &str) -> String {
        fill(self.image_load_failed, error)
    }

    /// "397 fontes instaladas".
    pub fn fonts_count(&self, n: usize) -> String {
        fill(self.fonts_count, n)
    }

    /// "Letra 2 de 5".
    pub fn lyrics_choice(&self, i: usize, n: usize) -> String {
        self.lyrics_choice.replace("{i}", &i.to_string()).replace("{n}", &n.to_string())
    }

    pub fn cache_cleared(&self, freed: &str) -> String {
        fill(self.cache_cleared, freed)
    }

    pub fn login_spawn_failed(&self, error: &str) -> String {
        fill(self.login_spawn_failed, error)
    }

    pub fn ytm_no_connection(&self, error: &str) -> String {
        fill(self.ytm_no_connection, error)
    }

    pub fn ytm_status(&self, status: u16) -> String {
        fill(self.ytm_status, status)
    }

    pub fn ytm_unexpected(&self, error: &str) -> String {
        fill(self.ytm_unexpected, error)
    }

    pub fn playlist_answered(&self, status: &str) -> String {
        fill(self.playlist_answered, status)
    }

    pub fn queue_error(&self, error: &str) -> String {
        fill(self.queue_error, error)
    }

    pub fn no_audio_output(&self, error: &str) -> String {
        fill(self.no_audio_output, error)
    }

    pub fn ytdlp_install_failed(&self, error: &str) -> String {
        fill(self.ytdlp_install_failed, error)
    }

    pub fn audio_server_answered(&self, status: impl std::fmt::Display) -> String {
        fill(self.audio_server_answered, status)
    }

    pub fn download_failed(&self, error: &str) -> String {
        fill(self.download_failed, error)
    }

    pub fn download_interrupted(&self, error: &str) -> String {
        fill(self.download_interrupted, error)
    }

    pub fn audio_format_unknown(&self, error: &str) -> String {
        fill(self.audio_format_unknown, error)
    }

    pub fn audio_no_decoder(&self, error: &str) -> String {
        fill(self.audio_no_decoder, error)
    }

    pub fn download_failed_retry(&self, error: &str) -> String {
        fill(self.download_failed_retry, error)
    }

    /// "1 música" ou "12 músicas", e que tocam sem internet.
    pub fn downloads_count(&self, n: usize) -> String {
        if n == 1 { self.downloads_one.to_owned() } else { fill(self.downloads_many, n) }
    }

    /// A dica da foto de outra conta.
    pub fn switch_to(&self, name: &str) -> String {
        fill(self.switch_to, name)
    }

    pub fn now_in_account(&self, name: &str) -> String {
        fill(self.now_in_account, name)
    }

    /// "1 música" ou "12 músicas".
    pub fn songs_count(&self, n: usize) -> String {
        if n == 1 { self.songs_one.to_owned() } else { fill(self.songs_many, n) }
    }

    /// "1 álbum" ou "3 álbuns".
    pub fn albums_count(&self, n: usize) -> String {
        if n == 1 { self.albums_one.to_owned() } else { fill(self.albums_many, n) }
    }

    pub fn delete_download_failed(&self, error: &str) -> String {
        fill(self.delete_download_failed, error)
    }

    pub fn drag_out_failed(&self, error: &str) -> String {
        fill(self.drag_out_failed, error)
    }

    pub fn saving_to(&self, dir: &str) -> String {
        fill(self.saving_to, dir)
    }

    pub fn folder_unusable(&self, error: &str) -> String {
        fill(self.folder_unusable, error)
    }
}
