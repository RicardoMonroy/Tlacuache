//! Medir el contenido de archivos y carpetas (propiedades; más adelante la
//! vista previa de carpetas). Recorre sin guardar la lista en memoria, así
//! que sirve también para carpetas enormes, y se puede cancelar.

use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{gio, glib};

const ATTRS: &str = "standard::name,standard::type,standard::size";
const REPORT_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Measure {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

/// Suma archivos, carpetas (sin contar las raíces) y bytes de `roots`, sin
/// seguir enlaces. Las subcarpetas que no se pueden leer se omiten. Llama a
/// `on_progress` cada cierto tiempo con el acumulado.
pub async fn measure<F>(
    roots: &[gio::File],
    cancellable: &gio::Cancellable,
    on_progress: F,
) -> Result<Measure, glib::Error>
where
    F: Fn(&Measure),
{
    let flags = gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS;
    let priority = glib::Priority::DEFAULT;
    let mut total = Measure::default();
    let mut pending = Vec::new();
    for root in roots {
        let info = root.query_info_future(ATTRS, flags, priority).await?;
        if info.file_type() == gio::FileType::Directory {
            pending.push(root.clone());
        } else {
            total.files += 1;
            total.bytes += u64::try_from(info.size()).unwrap_or(0);
        }
    }

    let mut last_report = Instant::now();
    while let Some(dir) = pending.pop() {
        if cancellable.is_cancelled() {
            return Err(glib::Error::new(gio::IOErrorEnum::Cancelled, ""));
        }
        let Ok(enumerator) = dir.enumerate_children_future(ATTRS, flags, priority).await else {
            continue;
        };
        while let Ok(batch) = enumerator.next_files_future(512, priority).await {
            if batch.is_empty() {
                break;
            }
            for info in batch {
                if info.file_type() == gio::FileType::Directory {
                    total.dirs += 1;
                    pending.push(dir.child(info.name()));
                } else {
                    total.files += 1;
                    total.bytes += u64::try_from(info.size()).unwrap_or(0);
                }
            }
            if last_report.elapsed() >= REPORT_INTERVAL {
                on_progress(&total);
                last_report = Instant::now();
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn measures_files_folders_and_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("a/b")).unwrap();
        fs::write(tmp.path().join("a/uno.txt"), vec![0u8; 100]).unwrap();
        fs::write(tmp.path().join("a/b/dos.txt"), vec![0u8; 50]).unwrap();
        let suelto = tmp.path().join("suelto.bin");
        fs::write(&suelto, vec![0u8; 7]).unwrap();

        let context = glib::MainContext::new();
        let roots = [
            gio::File::for_path(tmp.path().join("a")),
            gio::File::for_path(&suelto),
        ];
        let result = context
            .with_thread_default(|| {
                context.block_on(measure(&roots, &gio::Cancellable::new(), |_| {}))
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            result,
            Measure {
                files: 3,
                dirs: 1,
                bytes: 157
            }
        );
    }
}
