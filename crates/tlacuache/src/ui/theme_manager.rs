//! Aplica el tema activo (`docs/THEMES.md` §5.3):
//!
//! 1. `base.css` (sin colores) en un `CssProvider` de prioridad APPLICATION.
//! 2. Las variables `--tl-*` del tema en un segundo provider de prioridad
//!    mayor; cambiar de tema solo recarga este.
//! 3. Claro/oscuro de `adw::StyleManager` según la variante del tema, o
//!    siguiendo al sistema con `follow_system` (tema `light`/`dark`).
//! 4. `theme-changed` para lo que no es CSS: clases de flags en la ventana
//!    y colores de la terminal.
//! 5. El esquema de GtkSourceView del tema (ADR-011) se escribe en
//!    `~/.cache/tlacuache/styles/` en un hilo de trabajo; al quedar listo se
//!    emite `source-scheme-changed`.

use std::cell::{OnceCell, RefCell};
use std::path::PathBuf;
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gdk, gio, glib};
use tlacuache_core::config;
use tlacuache_core::theme::{AgeStyle, IconStyle, SelectionStyle, Theme, Variant};

const BASE_CSS: &str = "/io/github/rmonroy/Tlacuache/style/base.css";

/// Clases de la ventana que corresponden a flags de `[style]`.
const FLAG_CLASSES: [&str; 7] = [
    "tl-mono",
    "tl-flat",
    "tl-glow",
    "tl-uppercase",
    "tl-select-invert",
    "tl-age-text",
    "tl-icons-symbolic",
];

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ThemeManager {
        pub base: gtk::CssProvider,
        pub variables: gtk::CssProvider,
        pub settings: RefCell<config::Theme>,
        pub current: RefCell<Option<Theme>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ThemeManager {
        const NAME: &'static str = "TlacuacheThemeManager";
        type Type = super::ThemeManager;
    }

    impl ObjectImpl for ThemeManager {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("theme-changed").build(),
                    Signal::builder("source-scheme-changed").build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct ThemeManager(ObjectSubclass<imp::ThemeManager>);
}

thread_local! {
    /// Uno por app (hilo de GTK). Se crea en `startup`.
    static MANAGER: OnceCell<ThemeManager> = const { OnceCell::new() };
}

/// El gestor de temas de la app, si ya se inició.
pub fn get() -> Option<ThemeManager> {
    MANAGER.with(|m| m.get().cloned())
}

/// Crea el gestor, registra los estilos en `display` y aplica el tema de
/// la config. Llamar una vez, en `startup`.
pub fn init(settings: config::Theme, display: &gdk::Display) -> ThemeManager {
    let manager: ThemeManager = glib::Object::new();
    manager.install(settings, display);
    MANAGER.with(|m| {
        let _ = m.set(manager.clone());
    });
    manager
}

impl ThemeManager {
    pub fn connect_theme_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "theme-changed",
            false,
            glib::closure_local!(move |manager: &Self| f(manager)),
        )
    }

    pub fn connect_source_scheme_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "source-scheme-changed",
            false,
            glib::closure_local!(move |manager: &Self| f(manager)),
        );
    }

    /// Esquema de GtkSourceView del tema aplicado, si ya está escrito.
    pub fn source_scheme(&self) -> Option<sourceview5::StyleScheme> {
        sourceview5::StyleSchemeManager::default().scheme(&self.theme().source_scheme_id())
    }

    /// Tema aplicado (el predeterminado si aún no hay ninguno).
    pub fn theme(&self) -> Theme {
        self.imp()
            .current
            .borrow()
            .clone()
            .unwrap_or_else(Theme::default_theme)
    }

    fn install(&self, settings: config::Theme, display: &gdk::Display) {
        let imp = self.imp();
        imp.settings.replace(settings);
        imp.base.load_from_resource(BASE_CSS);
        gtk::style_context_add_provider_for_display(
            display,
            &imp.base,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        // Mayor prioridad que `base.css`: las variables siempre ganan.
        gtk::style_context_add_provider_for_display(
            display,
            &imp.variables,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );

        sourceview5::StyleSchemeManager::default()
            .prepend_search_path(&styles_dir().to_string_lossy());

        // Con `follow_system`, cambiar el modo del sistema cambia de tema.
        adw::StyleManager::default().connect_dark_notify(glib::clone!(
            #[weak(rename_to = manager)]
            self,
            move |_| {
                if manager.imp().settings.borrow().follow_system {
                    manager.apply();
                }
            }
        ));
        self.apply();
    }

    /// Resuelve el tema de la config y lo aplica.
    pub fn apply(&self) {
        let imp = self.imp();
        let style = adw::StyleManager::default();
        let settings = imp.settings.borrow().clone();
        if settings.follow_system {
            style.set_color_scheme(adw::ColorScheme::Default);
        }
        let id = settings.resolve(style.is_dark()).to_owned();
        let theme = Theme::builtin(&id).unwrap_or_else(|| {
            tracing::warn!("tema desconocido «{id}»; se usa el predeterminado");
            Theme::default_theme()
        });
        if !settings.follow_system {
            style.set_color_scheme(match theme.meta.variant {
                Variant::Dark => adw::ColorScheme::ForceDark,
                Variant::Light => adw::ColorScheme::ForceLight,
            });
        }
        imp.variables.load_from_string(&theme.css_variables());
        tracing::debug!("tema aplicado: {}", theme.meta.id);
        let (file_name, xml) = (
            format!("{}.xml", theme.source_scheme_id()),
            theme.source_scheme(),
        );
        imp.current.replace(Some(theme));
        self.emit_by_name::<()>("theme-changed", &[]);
        self.write_source_scheme(file_name, xml);
    }

    fn write_source_scheme(&self, file_name: String, xml: String) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = manager)]
            self,
            async move {
                let written = gio::spawn_blocking(move || {
                    let dir = styles_dir();
                    std::fs::create_dir_all(&dir)?;
                    std::fs::write(dir.join(file_name), xml)
                })
                .await;
                match written {
                    Ok(Ok(())) => {
                        sourceview5::StyleSchemeManager::default().force_rescan();
                        manager.emit_by_name::<()>("source-scheme-changed", &[]);
                    }
                    Ok(Err(err)) => {
                        tracing::warn!("no se pudo escribir el esquema de sintaxis: {err}");
                    }
                    Err(_) => tracing::warn!("falló el hilo que escribe el esquema de sintaxis"),
                }
            }
        ));
    }
}

/// Carpeta de los esquemas de GtkSourceView generados.
fn styles_dir() -> PathBuf {
    glib::user_cache_dir().join("tlacuache").join("styles")
}

/// Pone en `widget` (la ventana) las clases de los flags de `[style]` del
/// tema y quita las demás.
pub fn apply_flag_classes(widget: &impl IsA<gtk::Widget>, theme: &Theme) {
    let style = &theme.style;
    let active = [
        style.mono_ui,
        style.flat,
        style.glow,
        style.uppercase_headers,
        style.selection_style == SelectionStyle::Invert,
        style.age_style == AgeStyle::Text,
        style.icon_style == IconStyle::Symbolic,
    ];
    for (class, on) in FLAG_CLASSES.into_iter().zip(active) {
        if on {
            widget.add_css_class(class);
        } else {
            widget.remove_css_class(class);
        }
    }
}
