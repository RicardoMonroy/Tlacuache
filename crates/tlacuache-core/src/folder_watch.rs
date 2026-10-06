//! Carpetas que no avisan de sus cambios (R.4): montajes de red o FUSE
//! (OneDrive, sshfs, rclone, SFTP por GVfs). Su monitor no existe o solo ve
//! los cambios hechos desde este equipo, así que se revisan periódicamente
//! mientras están a la vista.

use std::time::Duration;

/// Revisión más frecuente.
pub const MIN_POLL_INTERVAL: Duration = Duration::from_secs(3);
/// Revisión menos frecuente (carpetas muy lentas de leer).
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// ¿Hay que revisar la carpeta periódicamente? `monitored`: se pudo crear
/// su monitor; `remote` y `fs_type`: `filesystem::remote` y
/// `filesystem::type` de gio.
///
/// FUSE se trata como remoto: gio lo informa como `fuse` sin marcarlo
/// remoto (p. ej. onedriver), y sus cambios suelen venir de otro equipo.
/// `fuseblk` es un disco local (NTFS con ntfs-3g) y sí avisa.
pub fn needs_polling(monitored: bool, remote: bool, fs_type: Option<&str>) -> bool {
    let fuse = fs_type.is_some_and(|t| t == "fuse" || t.starts_with("fuse."));
    !monitored || remote || fuse
}

/// Pausa hasta la siguiente revisión: diez veces lo que tardó la última
/// lectura, entre `MIN_POLL_INTERVAL` y `MAX_POLL_INTERVAL`, para que una
/// carpeta lenta o enorme no pase el tiempo leyéndose.
pub fn poll_interval(last_read: Duration) -> Duration {
    last_read
        .saturating_mul(10)
        .clamp(MIN_POLL_INTERVAL, MAX_POLL_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_folders_with_monitor_are_not_polled() {
        for fs in ["ext4", "btrfs", "tmpfs", "xfs", "fuseblk"] {
            assert!(!needs_polling(true, false, Some(fs)), "{fs}");
        }
        assert!(!needs_polling(true, false, None));
    }

    #[test]
    fn fuse_remote_or_unmonitored_folders_are_polled() {
        assert!(needs_polling(true, false, Some("fuse")));
        assert!(needs_polling(true, false, Some("fuse.sshfs")));
        assert!(needs_polling(true, false, Some("fuse.onedriver")));
        assert!(needs_polling(true, true, Some("nfs")));
        assert!(needs_polling(true, true, None));
        assert!(needs_polling(false, false, Some("ext4")));
    }

    #[test]
    fn interval_scales_with_read_time_within_bounds() {
        assert_eq!(poll_interval(Duration::ZERO), MIN_POLL_INTERVAL);
        assert_eq!(poll_interval(Duration::from_millis(50)), MIN_POLL_INTERVAL);
        assert_eq!(
            poll_interval(Duration::from_millis(800)),
            Duration::from_secs(8)
        );
        assert_eq!(poll_interval(Duration::from_secs(10)), MAX_POLL_INTERVAL);
        assert_eq!(poll_interval(Duration::MAX), MAX_POLL_INTERVAL);
    }
}
