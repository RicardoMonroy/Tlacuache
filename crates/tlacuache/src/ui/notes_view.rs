//! Editor de la nota de una carpeta (8.8), dentro de la vista previa. Se
//! guarda solo poco después de dejar de escribir, y al cambiar de carpeta o
//! cerrar la vista previa (`flush`). Vaciarla borra la nota.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::fs::notes;
use crate::strings;

/// Espera tras la última tecla antes de guardar.
const SAVE_DELAY: Duration = Duration::from_millis(600);

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct NotesView {
        pub title: gtk::Label,
        pub text: gtk::TextView,
        pub placeholder: gtk::Label,
        /// Carpeta cuya nota se edita.
        pub uri: RefCell<Option<String>>,
        /// Cargando: los cambios del búfer no son del usuario.
        pub loading: Cell<bool>,
        /// Hay cambios sin guardar.
        pub dirty: Cell<bool>,
        pub save_timer: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NotesView {
        const NAME: &'static str = "TlacuacheNotesView";
        type Type = super::NotesView;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for NotesView {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn dispose(&self) {
            self.obj().flush();
        }
    }
    impl WidgetImpl for NotesView {}
    impl BoxImpl for NotesView {}
}

glib::wrapper! {
    pub struct NotesView(ObjectSubclass<imp::NotesView>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for NotesView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl NotesView {
    fn build(&self) {
        let imp = self.imp();
        self.set_orientation(gtk::Orientation::Vertical);
        self.set_spacing(6);
        self.add_css_class("tl-notes");

        imp.title.set_xalign(0.0);
        imp.title.add_css_class("tl-preview-title");

        imp.text.set_wrap_mode(gtk::WrapMode::WordChar);
        imp.text.set_left_margin(8);
        imp.text.set_right_margin(8);
        imp.text.set_top_margin(8);
        imp.text.set_bottom_margin(8);
        imp.text.add_css_class("tl-notes-text");
        imp.text.buffer().connect_changed(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move |buffer| {
                let imp = view.imp();
                imp.placeholder.set_visible(buffer.char_count() == 0);
                if !imp.loading.get() {
                    imp.dirty.set(true);
                    view.schedule_save();
                }
            }
        ));

        imp.placeholder.set_text(strings::NOTES_PLACEHOLDER);
        imp.placeholder.add_css_class("dim-label");
        imp.placeholder.set_halign(gtk::Align::Start);
        imp.placeholder.set_valign(gtk::Align::Start);
        imp.placeholder.set_margin_start(10);
        imp.placeholder.set_margin_top(8);
        imp.placeholder.set_can_target(false);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&imp.text)
            .build();
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&scrolled));
        overlay.add_overlay(&imp.placeholder);

        self.append(&imp.title);
        self.append(&overlay);
    }

    /// Muestra la nota de la carpeta `uri` (con su nombre visible), o nada.
    /// Antes guarda lo pendiente de la anterior.
    pub fn set_folder(&self, folder: Option<(String, String)>) {
        self.flush();
        let imp = self.imp();
        imp.loading.set(true);
        imp.text.buffer().set_text("");
        imp.loading.set(false);
        let Some((uri, name)) = folder else {
            imp.uri.replace(None);
            return;
        };
        imp.title.set_text(&strings::notes_title(&name));
        imp.uri.replace(Some(uri.clone()));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = view)]
            self,
            async move {
                let text = notes::get().load(&uri).await;
                let imp = view.imp();
                // Siguió en la misma carpeta y no se escribió mientras.
                if imp.uri.borrow().as_deref() == Some(uri.as_str()) && !imp.dirty.get() {
                    imp.loading.set(true);
                    imp.text.buffer().set_text(&text);
                    imp.loading.set(false);
                }
            }
        ));
    }

    fn schedule_save(&self) {
        let imp = self.imp();
        if let Some(id) = imp.save_timer.take() {
            id.remove();
        }
        let id = glib::timeout_add_local_once(
            SAVE_DELAY,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                move || {
                    view.imp().save_timer.take();
                    view.save_now();
                }
            ),
        );
        imp.save_timer.replace(Some(id));
    }

    /// Guarda ya lo pendiente (al cambiar de carpeta o cerrar).
    pub fn flush(&self) {
        let imp = self.imp();
        if let Some(id) = imp.save_timer.take() {
            id.remove();
        }
        if imp.dirty.get() {
            self.save_now();
        }
    }

    fn save_now(&self) {
        let imp = self.imp();
        imp.dirty.set(false);
        let Some(uri) = imp.uri.borrow().clone() else {
            return;
        };
        let buffer = imp.text.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        glib::spawn_future_local(async move {
            notes::get().save(&uri, &text).await;
        });
    }
}
