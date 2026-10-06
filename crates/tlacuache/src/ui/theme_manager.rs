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
use tlacuache_core::theme_catalog::ThemeCatalog;

use crate::fs::config_file;
use crate::{strings, window};
use tlacuache_core::theme::{AgeStyle, IconStyle, SelectionStyle, Theme, Variant};

/// Respaldo si el tema elegido no existe.
const DEFAULT_ID: &str = "nord";
const BASE_CSS: &str = "/io/github/RicardoMonroy/Tlacuache/style/base.css";

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
        pub catalog: RefCell<ThemeCatalog>,
        /// Avisos para mostrar cuando haya ventana.
        pub pending: RefCell<Vec<String>>,
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
pub fn init(
    settings: config::Theme,
    catalog: ThemeCatalog,
    display: &gdk::Display,
) -> ThemeManager {
    let manager: ThemeManager = glib::Object::new();
    for error in catalog.errors() {
        manager.notify_user(strings::theme_load_failed(&error.path, &error.message));
    }
    manager.imp().catalog.replace(catalog);
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

    /// Temas disponibles (incluidos y de usuario), en orden de presentación.
    pub fn themes(&self) -> Vec<Theme> {
        self.imp().catalog.borrow().themes().to_vec()
    }

    /// Avisa con un toast en la ventana activa; si aún no hay ventana (al
    /// arrancar), queda pendiente para `take_pending_notices`.
    fn notify_user(&self, text: String) {
        let window = gio::Application::default()
            .and_downcast::<gtk::Application>()
            .and_then(|app| app.active_window());
        match window {
            Some(window) => window::show_toast_from(&window, &text),
            None => self.imp().pending.borrow_mut().push(text),
        }
    }

    /// Avisos generados antes de que existiera la ventana.
    pub fn take_pending_notices(&self) -> Vec<String> {
        self.imp().pending.take()
    }

    /// `[theme]` actual.
    pub fn settings(&self) -> config::Theme {
        self.imp().settings.borrow().clone()
    }

    /// Cambia el tema en vivo (Preferencias, menú) y lo guarda en
    /// config.toml. Un error al guardar se avisa con un toast.
    pub fn set_settings(&self, settings: config::Theme) {
        if *self.imp().settings.borrow() == settings {
            return;
        }
        self.imp().settings.replace(settings.clone());
        self.apply();
        glib::spawn_future_local(async move {
            let path = crate::app::config_path();
            let result =
                config_file::update(&path, |text| config::update_theme_toml(text, &settings)).await;
            if let Err(err) = result {
                tracing::warn!("no se guardó el tema: {err}");
                let window = gio::Application::default()
                    .and_downcast::<gtk::Application>()
                    .and_then(|app| app.active_window());
                if let Some(window) = window {
                    window::show_toast_from(&window, &strings::theme_save_failed(&err));
                }
            }
        });
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
        let catalog = imp.catalog.borrow();
        let theme = match catalog.get(&id) {
            Some(theme) => theme.clone(),
            None => {
                tracing::warn!("tema desconocido «{id}»; se usa el predeterminado");
                self.notify_user(strings::theme_missing(&id));
                catalog
                    .get(DEFAULT_ID)
                    .cloned()
                    .unwrap_or_else(Theme::default_theme)
            }
        };
        // Un tema de usuario se aplica aunque no pase la validación, pero se
        // avisa (una vez cada vez que pasa a ser el activo).
        let previous = imp.current.borrow().as_ref().map(|t| t.meta.id.clone());
        if catalog.is_user(&theme.meta.id) && previous.as_deref() != Some(theme.meta.id.as_str()) {
            let issues = theme.validate();
            if !issues.is_empty() {
                self.notify_user(strings::theme_contrast(&theme.meta.name, &issues));
            }
        }
        drop(catalog);
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
