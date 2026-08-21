#![windows_subsystem = "windows"]

use gpui::{
    px, size, App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions,
};

mod ai;
mod assets;
mod audio_fx;
mod calendar;
mod components;
mod cues;
mod cutout;
mod dialogs;
mod edit;
mod effects_ui;
mod export;
mod fileassoc;
mod gizmos;
mod graph_editor;
mod home;
mod input;
mod interaction;
mod keybindings;
mod launch;
mod library;
mod library_ui;
mod lut;
mod motion;
mod notify;
mod panels;
mod playback;
mod preview;
mod preview_audio;
mod projects;
mod properties;
mod reframe;
mod scroll;
mod settings_ui;
mod shell;
mod shortcuts;
mod sounds_ui;
mod stabilize;
mod state;
mod stickers_ui;
mod templates_ui;
mod text;
mod text_anim;
mod theme;
mod titlebar;
mod tracking;
mod youtube_ui;

use assets::Icons;
use shell::Shell;

const FONT_FAMILY: &str = "Inter";
const FONT_FALLBACK: &str = "Segoe UI";

const INTER_VARIABLE: &[u8] = include_bytes!("../assets/fonts/InterVariable.ttf");

fn preferred_font(cx: &App) -> SharedString {
    let _ = cx
        .text_system()
        .add_fonts(vec![std::borrow::Cow::Borrowed(INTER_VARIABLE)]);

    if cx
        .text_system()
        .all_font_names()
        .iter()
        .any(|name| name == FONT_FAMILY)
    {
        SharedString::from(FONT_FAMILY)
    } else {
        SharedString::from(FONT_FALLBACK)
    }
}

fn locale_code(raw: &str) -> Option<String> {
    let code = raw
        .split(['.', '_', '-'])
        .next()
        .unwrap_or(raw)
        .trim()
        .to_ascii_lowercase();
    (!code.is_empty()).then_some(code)
}

#[cfg(windows)]
fn system_locale() -> Option<String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0u16; 85];
    let written = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if written <= 1 {
        return None;
    }
    let name = String::from_utf16_lossy(&buffer[..(written as usize - 1)]);
    locale_code(&name)
}

#[cfg(not(windows))]
fn system_locale() -> Option<String> {
    None
}

fn detected_locale(saved: Option<String>) -> Option<String> {
    saved.or_else(|| {
        ["CUTIX_LOCALE", "LC_ALL", "LANG"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok())
            .find_map(|raw| locale_code(&raw))
            .or_else(system_locale)
    })
}

fn shell_label(key: &str) -> String {
    let system = detected_locale(None).unwrap_or_else(|| cutix_i18n::BASE_LOCALE.to_string());
    let translated = cutix_i18n::translate(&system, key);
    if translated == key {
        return cutix_i18n::translate(cutix_i18n::BASE_LOCALE, key);
    }
    translated
}

fn adopt_previous_data() {
    for base in [dirs::data_dir(), dirs::data_local_dir()]
        .into_iter()
        .flatten()
    {
        let previous = base.join("OpenCut");
        let current = base.join("cutix");
        if previous.is_dir() && !current.exists() {
            let _ = std::fs::rename(&previous, &current);
        }
    }
}

fn main() {
    adopt_previous_data();
    cutix_i18n::bootstrap();
    let settings = state::load_settings();
    if let Some(code) = detected_locale(settings.locale.clone()) {
        cutix_i18n::set_locale(&code);
    }

    let launch = launch::from_env();
    match &launch {
        launch::Launch::Register => {
            report(fileassoc::register(&[
                fileassoc::Group::Video,
                fileassoc::Group::Audio,
            ]));
            return;
        }
        launch::Launch::Unregister => {
            report(fileassoc::unregister());
            return;
        }
        launch::Launch::NotifyCheck => {
            let copied = notify::copy_link("https://youtu.be/clipboard-check");
            let shown = notify::published("cutix", "https://youtu.be/example");
            let _ = copied;
            let path = std::env::temp_dir().join("cutix-notify-check.txt");
            let _ = std::fs::write(
                &path,
                format!(
                    "shown = {shown}
"
                ),
            );
            std::thread::sleep(std::time::Duration::from_secs(12));
            return;
        }
        _ => {}
    }

    let _ = fileassoc::register_folder_verb(&shell_label("library.openHere"));

    let _ = fileassoc::register(&[fileassoc::Group::Video]);
    let _ = fileassoc::register_file_verbs([
        &shell_label("library.editHere"),
        &shell_label("library.publishHere"),
    ]);

    let browse = match &launch {
        launch::Launch::BrowseFolder(path) => Some(path.clone()),
        _ => None,
    };
    let publish = match &launch {
        launch::Launch::Publish(path) => Some(path.clone()),
        _ => None,
    };
    let open_media = match &launch {
        launch::Launch::OpenMedia(path) => Some(path.clone()),
        _ => None,
    };
    let dark = settings.dark.unwrap_or(true);
    let store = cutix_project::ProjectStore::with_app_data_directory()
        .unwrap_or_else(|_| cutix_project::ProjectStore::new(std::env::temp_dir().join("cutix")));

    Application::new()
        .with_assets(Icons)
        .run(move |cx: &mut App| {
            let font = preferred_font(cx);
            let saved = settings.window.filter(|geometry| geometry.is_sane());
            let bounds = match saved {
                Some(geometry) => {
                    let display = cx
                        .primary_display()
                        .map(|display| {
                            let bounds = display.bounds();
                            (
                                f32::from(bounds.origin.x),
                                f32::from(bounds.origin.y),
                                f32::from(bounds.origin.x + bounds.size.width),
                                f32::from(bounds.origin.y + bounds.size.height),
                            )
                        })
                        .unwrap_or((0.0, 0.0, 8192.0, 8192.0));
                    let geometry = geometry.clamped(display);
                    Bounds {
                        origin: gpui::point(px(geometry.x), px(geometry.y)),
                        size: size(px(geometry.width), px(geometry.height)),
                    }
                }
                None => Bounds::centered(None, size(px(1280.), px(800.)), cx),
            };

            let publishing = publish.is_some();
            let window_bounds = if publishing {
                WindowBounds::Windowed(Bounds::centered(None, size(px(1000.), px(560.)), cx))
            } else {
                match saved {
                    Some(geometry) if !geometry.maximized => WindowBounds::Windowed(bounds),
                    _ => WindowBounds::Maximized(bounds),
                }
            };

            cx.open_window(
                WindowOptions {
                    titlebar: (!publishing).then(|| TitlebarOptions {
                        title: Some(SharedString::from("cutix")),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),

                    window_decorations: publishing.then_some(gpui::WindowDecorations::Client),
                    window_background: if publishing {
                        gpui::WindowBackgroundAppearance::Transparent
                    } else {
                        gpui::WindowBackgroundAppearance::Opaque
                    },
                    window_bounds: Some(window_bounds),
                    window_min_size: Some(if publishing {
                        size(px(420.), px(220.))
                    } else {
                        size(px(960.), px(600.))
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    window.set_rem_size(px(theme::REM));
                    crate::youtube_ui::render::set_chromeless(publishing);
                    let app = state::shared(store, dark, cx);
                    if let Some(directory) = browse.clone() {
                        app.update(cx, |model, cx| model.browse_videos(directory, cx));
                    }
                    if let Some(path) = publish.clone() {
                        app.update(cx, |model, cx| {
                            model.youtube_request = Some(path);
                            cx.notify();
                        });
                    }
                    if let Some(path) = open_media.clone() {
                        let name = launch::project_name(&path);
                        app.update(cx, |model, cx| model.open_video_as_project(path, name, cx));
                    }
                    let shell = cx.new(|cx| {
                        let mut shell = Shell::new(app, font, cx);
                        shell.publish_only = publishing;
                        shell
                    });
                    shell.update(cx, |shell, cx| shell.focus(window, cx));
                    let closing = shell.downgrade();
                    window.on_window_should_close(cx, move |_, cx| {
                        if let Some(shell) = closing.upgrade() {
                            shell.read(cx).flush_geometry();

                            let app = shell.read(cx).app.clone();
                            app.update(cx, |model, _| model.save_now());
                        }
                        true
                    });
                    shell
                },
            )
            .expect("failed to open the main window");

            cx.activate(true);
        });
}

fn report(result: Result<(), String>) {
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{detected_locale, locale_code};

    #[test]
    fn a_saved_choice_overrides_the_system_locale() {
        assert_eq!(detected_locale(Some("uk".into())).as_deref(), Some("uk"));
    }

    #[test]
    fn windows_style_locales_reduce_to_a_language_code() {
        assert_eq!(locale_code("ru_RU.UTF-8").as_deref(), Some("ru"));
        assert_eq!(locale_code("uk-UA").as_deref(), Some("uk"));
        assert_eq!(locale_code("en").as_deref(), Some("en"));
    }

    #[test]
    fn empty_locale_is_ignored() {
        assert!(locale_code("").is_none());
    }
}
