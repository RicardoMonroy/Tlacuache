//! Barra lateral: punto de partida de la navegación. Secciones «Lugares»
//! (Inicio, carpetas XDG, Papelera) y «Unidades» (sistema, volúmenes y
//! montajes de `gio::VolumeMonitor`); favoritos llega en la tarea 3.4.

use std::cell::RefCell;
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gio, glib, pango};
use tlacuache_core::places::{self, PlaceKind, PlaceTarget};

use crate::strings;
use crate::ui::drive_row::{DriveKind, DriveRow};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Sidebar {
        pub places: gtk::ListBox,
        pub drives: gtk::ListBox,
        /// Se conserva para seguir recibiendo sus señales.
        pub volume_monitor: RefCell<Option<gio::VolumeMonitor>>,
        /// Destino de cada fila de `places`, por índice.
        pub place_files: RefCell<Vec<gio::File>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sidebar {
        const NAME: &'static str = "TlacuacheSidebar";
        type Type = super::Sidebar;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for Sidebar {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // Clic o Enter en un lugar: navegar en el panel activo.
                    Signal::builder("place-activated")
                        .param_types([gio::File::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }
    }

    impl WidgetImpl for Sidebar {}
    impl BinImpl for Sidebar {}
}

glib::wrapper! {
    pub struct Sidebar(ObjectSubclass<imp::Sidebar>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Sidebar {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Sidebar {
    pub fn connect_place_activated<F: Fn(&Self, &gio::File) + 'static>(&self, f: F) {
        self.connect_closure(
            "place-activated",
            false,
            glib::closure_local!(move |sidebar: &Self, file: &gio::File| f(sidebar, file)),
        );
    }

    fn build(&self) {
        let imp = self.imp();
        self.add_css_class("tlacuache-sidebar");

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&section_heading(strings::SIDEBAR_PLACES));
        content.append(&imp.places);
        content.append(&section_heading(strings::SIDEBAR_DRIVES));
        content.append(&imp.drives);

        imp.places.add_css_class("navigation-sidebar");
        imp.places.set_selection_mode(gtk::SelectionMode::None);
        imp.places.connect_row_activated(glib::clone!(
            #[weak(rename_to = sidebar)]
            self,
            move |_, row| {
                let file = usize::try_from(row.index())
                    .ok()
                    .and_then(|i| sidebar.imp().place_files.borrow().get(i).cloned());
                if let Some(file) = file {
                    sidebar.emit_by_name::<()>("place-activated", &[&file]);
                }
            }
        ));
        self.fill_places();

        imp.drives.add_css_class("navigation-sidebar");
        imp.drives.set_selection_mode(gtk::SelectionMode::None);
        imp.drives.connect_row_activated(|_, row| {
            if let Some(row) = row.downcast_ref::<DriveRow>() {
                row.open();
            }
        });
        self.watch_volumes();

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();
        self.set_child(Some(&scrolled));
    }

    fn fill_places(&self) {
        let imp = self.imp();
        let list = places::places(&glib::home_dir(), |kind| {
            xdg_directory(kind).and_then(glib::user_special_dir)
        });
        let mut files = Vec::with_capacity(list.len());
        for place in list {
            let file = match &place.target {
                PlaceTarget::Path(path) => gio::File::for_path(path),
                PlaceTarget::Uri(uri) => gio::File::for_uri(uri),
            };
            let (icon, label) = place_appearance(place.kind);
            imp.places.append(&sidebar_row(icon, label));
            files.push(file);
        }
        imp.place_files.replace(files);
    }
}

impl Sidebar {
    fn watch_volumes(&self) {
        let monitor = gio::VolumeMonitor::get();
        // Cualquier cambio reconstruye la sección: son pocas filas.
        let rebuild = glib::clone!(
            #[weak(rename_to = sidebar)]
            self,
            move || sidebar.fill_drives()
        );
        let r = rebuild.clone();
        monitor.connect_volume_added(move |_, _| r());
        let r = rebuild.clone();
        monitor.connect_volume_removed(move |_, _| r());
        let r = rebuild.clone();
        monitor.connect_volume_changed(move |_, _| r());
        let r = rebuild.clone();
        monitor.connect_mount_added(move |_, _| r());
        let r = rebuild.clone();
        monitor.connect_mount_removed(move |_, _| r());
        monitor.connect_mount_changed(move |_, _| rebuild());
        self.imp().volume_monitor.replace(Some(monitor));
        self.fill_drives();
    }

    fn fill_drives(&self) {
        let imp = self.imp();
        let Some(monitor) = imp.volume_monitor.borrow().clone() else {
            return;
        };
        imp.drives.remove_all();

        let mut kinds = vec![DriveKind::Root];
        kinds.extend(monitor.volumes().into_iter().map(DriveKind::Volume));
        kinds.extend(
            monitor
                .mounts()
                .into_iter()
                .filter(|m| m.volume().is_none() && !m.is_shadowed())
                .map(DriveKind::Mount),
        );
        for kind in kinds {
            let row = DriveRow::new(kind);
            row.connect_open(glib::clone!(
                #[weak(rename_to = sidebar)]
                self,
                move |_, root| sidebar.emit_by_name::<()>("place-activated", &[root])
            ));
            imp.drives.append(&row);
        }
    }
}

fn xdg_directory(kind: PlaceKind) -> Option<glib::UserDirectory> {
    match kind {
        PlaceKind::Desktop => Some(glib::UserDirectory::Desktop),
        PlaceKind::Documents => Some(glib::UserDirectory::Documents),
        PlaceKind::Downloads => Some(glib::UserDirectory::Downloads),
        PlaceKind::Music => Some(glib::UserDirectory::Music),
        PlaceKind::Pictures => Some(glib::UserDirectory::Pictures),
        PlaceKind::Videos => Some(glib::UserDirectory::Videos),
        PlaceKind::Home | PlaceKind::Trash => None,
    }
}

fn place_appearance(kind: PlaceKind) -> (&'static str, &'static str) {
    match kind {
        PlaceKind::Home => ("user-home-symbolic", strings::PLACE_HOME),
        PlaceKind::Desktop => ("user-desktop-symbolic", strings::PLACE_DESKTOP),
        PlaceKind::Documents => ("folder-documents-symbolic", strings::PLACE_DOCUMENTS),
        PlaceKind::Downloads => ("folder-download-symbolic", strings::PLACE_DOWNLOADS),
        PlaceKind::Music => ("folder-music-symbolic", strings::PLACE_MUSIC),
        PlaceKind::Pictures => ("folder-pictures-symbolic", strings::PLACE_PICTURES),
        PlaceKind::Videos => ("folder-videos-symbolic", strings::PLACE_VIDEOS),
        PlaceKind::Trash => ("user-trash-symbolic", strings::PLACE_TRASH),
    }
}

fn section_heading(text: &str) -> gtk::Label {
    let label = gtk::Label::builder().label(text).xalign(0.0).build();
    label.add_css_class("heading");
    label.add_css_class("sidebar-heading");
    label
}

fn sidebar_row(icon: &str, text: &str) -> gtk::ListBoxRow {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&gtk::Image::from_icon_name(icon));
    row.append(
        &gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .build(),
    );
    gtk::ListBoxRow::builder().child(&row).build()
}
