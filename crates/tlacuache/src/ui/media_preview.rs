//! Video y audio de la vista previa con el backend GStreamer de GTK
//! (`gtk::MediaFile`). No se reproduce solo: al seleccionar un archivo no
//! debe sonar nada hasta pulsar ▶. Se detiene al cambiar de archivo o al
//! cerrar la vista previa (`stop`).

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use tlacuache_core::filetype::FileCategory;

use crate::ui::set_category_icon;

const ICON_SIZE: i32 = 96;
const PAGE_VIDEO: &str = "video";
const PAGE_AUDIO: &str = "audio";

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct MediaPreview {
        pub stack: gtk::Stack,
        pub video: gtk::Video,
        pub audio_icon: gtk::Image,
        pub controls: gtk::MediaControls,
        pub stream: RefCell<Option<gtk::MediaFile>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MediaPreview {
        const NAME: &'static str = "TlacuacheMediaPreview";
        type Type = super::MediaPreview;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for MediaPreview {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }

        fn dispose(&self) {
            self.obj().stop();
        }
    }
    impl WidgetImpl for MediaPreview {}
    impl BinImpl for MediaPreview {}
}

glib::wrapper! {
    pub struct MediaPreview(ObjectSubclass<imp::MediaPreview>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for MediaPreview {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl MediaPreview {
    fn build(&self) {
        let imp = self.imp();
        imp.video.set_autoplay(false);
        imp.video.set_vexpand(true);

        imp.audio_icon.set_pixel_size(ICON_SIZE);
        set_category_icon(&imp.audio_icon, FileCategory::Audio);
        imp.controls.set_hexpand(true);
        let audio = gtk::Box::new(gtk::Orientation::Vertical, 12);
        audio.set_valign(gtk::Align::Center);
        audio.add_css_class("tl-preview-audio");
        audio.append(&imp.audio_icon);
        audio.append(&imp.controls);

        imp.stack.add_named(&imp.video, Some(PAGE_VIDEO));
        imp.stack.add_named(&audio, Some(PAGE_AUDIO));
        self.set_child(Some(&imp.stack));
    }

    /// Prepara `file` (sin reproducir) y devuelve su flujo, para leer la
    /// duración y los errores cuando GStreamer lo tenga listo.
    pub fn open(&self, file: &gio::File, video: bool) -> gtk::MediaFile {
        self.stop();
        let imp = self.imp();
        let stream = gtk::MediaFile::for_file(file);
        if video {
            imp.video.set_media_stream(Some(&stream));
            imp.stack.set_visible_child_name(PAGE_VIDEO);
        } else {
            imp.controls.set_media_stream(Some(&stream));
            imp.stack.set_visible_child_name(PAGE_AUDIO);
        }
        imp.stream.replace(Some(stream.clone()));
        stream
    }

    /// El flujo mostrado ahora (para descartar avisos de uno anterior).
    pub fn is_current(&self, stream: &gtk::MediaFile) -> bool {
        self.imp().stream.borrow().as_ref() == Some(stream)
    }

    /// Detiene y suelta el flujo actual.
    pub fn stop(&self) {
        let imp = self.imp();
        if let Some(stream) = imp.stream.take() {
            stream.pause();
            stream.clear();
        }
        imp.video.set_media_stream(None::<&gtk::MediaStream>);
        imp.controls.set_media_stream(None::<&gtk::MediaStream>);
    }
}
