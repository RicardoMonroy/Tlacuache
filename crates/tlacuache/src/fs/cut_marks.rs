//! Archivos cortados (Ctrl+X) pendientes de pegar: las vistas los muestran
//! atenuados. Se limpian al pegarlos, al copiar o cortar otra cosa, al
//! cambiar el portapapeles desde otra app y con Esc.

use std::cell::{OnceCell, RefCell};
use std::collections::HashSet;
use std::sync::OnceLock;

use glib::subclass::Signal;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct CutMarks {
        pub uris: RefCell<HashSet<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CutMarks {
        const NAME: &'static str = "TlacuacheCutMarks";
        type Type = super::CutMarks;
    }

    impl ObjectImpl for CutMarks {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("changed").build()])
        }
    }
}

glib::wrapper! {
    pub struct CutMarks(ObjectSubclass<imp::CutMarks>);
}

thread_local! {
    static MARKS: OnceCell<CutMarks> = const { OnceCell::new() };
}

/// Las marcas de la app (hilo de GTK).
pub fn get() -> CutMarks {
    MARKS.with(|m| m.get_or_init(glib::Object::new).clone())
}

impl CutMarks {
    /// Marca `uris` como cortadas (reemplaza las anteriores).
    pub fn set(&self, uris: impl IntoIterator<Item = String>) {
        self.imp().uris.replace(uris.into_iter().collect());
        self.emit_by_name::<()>("changed", &[]);
    }

    /// Quita todas las marcas. Devuelve `true` si había alguna.
    pub fn clear(&self) -> bool {
        if self.imp().uris.borrow().is_empty() {
            return false;
        }
        self.imp().uris.borrow_mut().clear();
        self.emit_by_name::<()>("changed", &[]);
        true
    }

    pub fn contains(&self, uri: &str) -> bool {
        self.imp().uris.borrow().contains(uri)
    }

    pub fn connect_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            glib::closure_local!(move |marks: &Self| f(marks)),
        )
    }
}
