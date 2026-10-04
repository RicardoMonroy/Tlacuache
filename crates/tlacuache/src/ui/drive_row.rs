//! Fila de unidad en la barra lateral: icono, nombre, barra de uso y botón
//! de expulsar/desmontar. Clic derecho abre un menú con las acciones.
//!
//! Montar, desmontar y expulsar usan `gtk::MountOperation`, que pide
//! contraseñas o autorización (polkit) cuando hace falta.

use std::cell::{OnceCell, RefCell};
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::{gio, glib, pango};
use tlacuache_core::usage::{UsageLevel, usage_level, used_fraction};

use crate::{strings, window};

/// Qué representa la fila.
#[derive(Clone, Debug)]
pub enum DriveKind {
    /// Sistema de archivos raíz (`/`).
    Root,
    /// Volumen (montado o no) de `gio::VolumeMonitor`.
    Volume(gio::Volume),
    /// Montaje sin volumen asociado (p. ej. red).
    Mount(gio::Mount),
}

const USAGE_CLASSES: [&str; 2] = ["usage-warning", "usage-critical"];

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DriveRow {
        pub kind: RefCell<Option<DriveKind>>,
        pub icon: gtk::Image,
        pub name: gtk::Label,
        pub usage: gtk::LevelBar,
        pub eject: gtk::Button,
        pub menu: OnceCell<gtk::PopoverMenu>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DriveRow {
        const NAME: &'static str = "TlacuacheDriveRow";
        type Type = super::DriveRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action("drive.open", None, |row, _, _| row.open());
            klass.install_action("drive.mount", None, |row, _, _| row.open());
            klass.install_action("drive.unmount", None, |row, _, _| row.unmount(false));
            klass.install_action("drive.eject", None, |row, _, _| row.unmount(true));
        }
    }

    impl ObjectImpl for DriveRow {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // Abrir la raíz de la unidad (montándola si hace falta).
                    Signal::builder("open")
                        .param_types([gio::File::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }

        fn dispose(&self) {
            if let Some(menu) = self.menu.get() {
                menu.unparent();
            }
        }
    }

    impl WidgetImpl for DriveRow {}
    impl ListBoxRowImpl for DriveRow {}
}

glib::wrapper! {
    pub struct DriveRow(ObjectSubclass<imp::DriveRow>)
        @extends gtk::ListBoxRow, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl DriveRow {
    pub fn new(kind: DriveKind) -> Self {
        let row: Self = glib::Object::new();
        row.imp().kind.replace(Some(kind));
        row.refresh();
        row
    }

    pub fn connect_open<F: Fn(&Self, &gio::File) + 'static>(&self, f: F) {
        self.connect_closure(
            "open",
            false,
            glib::closure_local!(move |row: &Self, file: &gio::File| f(row, file)),
        );
    }

    fn setup(&self) {
        let imp = self.imp();

        imp.name.set_xalign(0.0);
        imp.name.set_ellipsize(pango::EllipsizeMode::End);
        imp.usage.set_min_value(0.0);
        imp.usage.set_max_value(1.0);
        // Los offsets por defecto (low/high/full) son de batería: «low» es
        // malo. Aquí el color lo deciden las clases de `UsageLevel`.
        for offset in ["low", "high", "full"] {
            imp.usage.remove_offset_value(Some(offset));
        }
        imp.usage.add_css_class("drive-usage");

        let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        text.append(&imp.name);
        text.append(&imp.usage);

        imp.eject.set_icon_name("media-eject-symbolic");
        imp.eject.add_css_class("flat");
        imp.eject.add_css_class("circular");
        imp.eject.set_valign(gtk::Align::Center);
        imp.eject.set_focusable(false);
        imp.eject.set_tooltip_text(Some(strings::DRIVE_UNMOUNT));
        imp.eject.connect_clicked(glib::clone!(
            #[weak(rename_to = row)]
            self,
            move |_| row.unmount(row.can_eject())
        ));

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        content.append(&imp.icon);
        content.append(&text);
        content.append(&imp.eject);
        self.set_child(Some(&content));

        let menu = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
        menu.set_parent(self);
        menu.set_has_arrow(false);
        let _ = imp.menu.set(menu);
        let right_click = gtk::GestureClick::new();
        right_click.set_button(gtk::gdk::BUTTON_SECONDARY);
        right_click.connect_pressed(glib::clone!(
            #[weak(rename_to = row)]
            self,
            move |_, _, x, y| row.show_menu(x, y)
        ));
        self.add_controller(right_click);
    }

    fn kind(&self) -> Option<DriveKind> {
        self.imp().kind.borrow().clone()
    }

    fn mount(&self) -> Option<gio::Mount> {
        match self.kind()? {
            DriveKind::Root => None,
            DriveKind::Volume(volume) => volume.get_mount(),
            DriveKind::Mount(mount) => Some(mount),
        }
    }

    /// Carpeta raíz si está accesible (la raíz del sistema o un montaje).
    fn target(&self) -> Option<gio::File> {
        match self.kind()? {
            DriveKind::Root => Some(gio::File::for_path("/")),
            _ => self.mount().map(|m| m.root()),
        }
    }

    fn can_mount(&self) -> bool {
        matches!(self.kind(), Some(DriveKind::Volume(v)) if v.get_mount().is_none() && v.can_mount())
    }

    fn can_unmount(&self) -> bool {
        self.mount().is_some_and(|m| m.can_unmount())
    }

    fn can_eject(&self) -> bool {
        self.mount().is_some_and(|m| m.can_eject())
    }

    fn display_name(&self) -> String {
        match self.kind() {
            Some(DriveKind::Root) | None => strings::DRIVE_SYSTEM.to_owned(),
            Some(DriveKind::Volume(volume)) => volume.name().to_string(),
            Some(DriveKind::Mount(mount)) => mount.name().to_string(),
        }
    }

    /// Actualiza icono, nombre, botón, acciones y uso de disco.
    pub fn refresh(&self) {
        let imp = self.imp();
        let icon = match self.kind() {
            Some(DriveKind::Volume(volume)) => Some(volume.symbolic_icon()),
            Some(DriveKind::Mount(mount)) => Some(mount.symbolic_icon()),
            Some(DriveKind::Root) | None => None,
        };
        match icon {
            Some(icon) => imp.icon.set_from_gicon(&icon),
            None => imp
                .icon
                .set_icon_name(Some("drive-harddisk-system-symbolic")),
        }
        imp.name.set_text(&self.display_name());
        imp.eject
            .set_visible(self.can_unmount() || self.can_eject());
        self.action_set_enabled("drive.mount", self.can_mount());
        self.action_set_enabled("drive.unmount", self.can_unmount());
        self.action_set_enabled("drive.eject", self.can_eject());
        self.refresh_usage();
    }

    fn refresh_usage(&self) {
        let imp = self.imp();
        let Some(root) = self.target() else {
            imp.usage.set_visible(false);
            self.set_tooltip_text(Some(strings::DRIVE_NOT_MOUNTED));
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = row)]
            self,
            async move {
                let attrs = format!(
                    "{},{}",
                    gio::FILE_ATTRIBUTE_FILESYSTEM_FREE,
                    gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE
                );
                let info = root
                    .query_filesystem_info_future(&attrs, glib::Priority::DEFAULT)
                    .await
                    .ok()
                    .filter(|info| info.has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE));
                let usage = &row.imp().usage;
                let Some(info) = info else {
                    usage.set_visible(false);
                    return;
                };
                let free = info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE);
                let size = info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_SIZE);
                let fraction = used_fraction(free, size);
                usage.set_value(fraction);
                usage.set_visible(true);
                for class in USAGE_CLASSES {
                    usage.remove_css_class(class);
                }
                match usage_level(fraction) {
                    UsageLevel::Normal => {}
                    UsageLevel::Warning => usage.add_css_class("usage-warning"),
                    UsageLevel::Critical => usage.add_css_class("usage-critical"),
                }
                row.set_tooltip_text(Some(&strings::free_space(free, size)));
            }
        ));
    }

    /// Abre la unidad; si es un volumen sin montar, lo monta primero.
    pub fn open(&self) {
        if let Some(root) = self.target() {
            self.emit_by_name::<()>("open", &[&root]);
            return;
        }
        let Some(DriveKind::Volume(volume)) = self.kind() else {
            return;
        };
        if !volume.can_mount() {
            return;
        }
        let operation = gtk::MountOperation::new(self.root().and_downcast_ref::<gtk::Window>());
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = row)]
            self,
            async move {
                let result = volume
                    .mount_future(gio::MountMountFlags::NONE, Some(&operation))
                    .await;
                match result {
                    Ok(()) => {
                        row.refresh();
                        if let Some(root) = row.target() {
                            row.emit_by_name::<()>("open", &[&root]);
                        }
                    }
                    Err(err) => row.report_error(&err, strings::drive_mount_failed),
                }
            }
        ));
    }

    /// Desmonta (o expulsa, si `eject` y la unidad lo permite).
    fn unmount(&self, eject: bool) {
        let Some(mount) = self.mount() else {
            return;
        };
        let operation = gtk::MountOperation::new(self.root().and_downcast_ref::<gtk::Window>());
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = row)]
            self,
            async move {
                let flags = gio::MountUnmountFlags::NONE;
                let result = if eject && mount.can_eject() {
                    mount
                        .eject_with_operation_future(flags, Some(&operation))
                        .await
                } else {
                    mount
                        .unmount_with_operation_future(flags, Some(&operation))
                        .await
                };
                match result {
                    // El monitor de volúmenes reconstruye la lista.
                    Ok(()) => row.refresh(),
                    Err(err) => row.report_error(&err, strings::drive_unmount_failed),
                }
            }
        ));
    }

    fn report_error(&self, err: &glib::Error, message: fn(&str, &glib::Error) -> String) {
        // Cancelar el diálogo de autenticación no es un error a mostrar.
        if err.matches(gio::IOErrorEnum::FailedHandled) || err.matches(gio::IOErrorEnum::Cancelled)
        {
            return;
        }
        tracing::warn!("operación de unidad fallida: {err}");
        window::show_toast_from(self, &message(&self.display_name(), err));
    }

    fn show_menu(&self, x: f64, y: f64) {
        let menu = gio::Menu::new();
        menu.append(Some(strings::DRIVE_OPEN), Some("drive.open"));
        if self.can_mount() {
            menu.append(Some(strings::DRIVE_MOUNT), Some("drive.mount"));
        }
        if self.can_unmount() {
            menu.append(Some(strings::DRIVE_UNMOUNT), Some("drive.unmount"));
        }
        if self.can_eject() {
            menu.append(Some(strings::DRIVE_EJECT), Some("drive.eject"));
        }
        let Some(popover) = self.imp().menu.get() else {
            return;
        };
        popover.set_menu_model(Some(&menu));
        // Coordenadas de f64 a píxeles enteros para el rectángulo de anclaje.
        let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover.set_pointing_to(Some(&rect));
        popover.popup();
    }
}
