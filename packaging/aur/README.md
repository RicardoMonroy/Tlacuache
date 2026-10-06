# Paquetes de AUR

| Paquete | Qué compila |
|---|---|
| `tlacuache-browser-git` | La última versión de `main`. |
| `tlacuache-browser` | La versión etiquetada (`v$pkgver`) desde el tarball de GitHub. |

Instalan `/usr/bin/tlacuache`, el `.desktop` (`io.github.RicardoMonroy.Tlacuache`), los íconos (hicolor scalable y symbolic), la licencia y la documentación. `check()` corre las pruebas del workspace.

## Probar localmente

```bash
cd packaging/aur/tlacuache-browser-git
makepkg -si          # compila, prueba e instala
```

## Publicar una versión

1. Subir el repositorio a `https://github.com/RicardoMonroy/TlacuacheBrowser`.
2. Etiquetar y publicar: `git tag v0.2.0 && git push origin v0.2.0`. El workflow `Release` construye el `.deb` y el `.rpm`, los prueba instalándolos y crea el release de GitHub con ambos adjuntos.
3. En `tlacuache-browser/`: `updpkgsums` (pone el SHA-256 real del tarball) y `makepkg --printsrcinfo > .SRCINFO`.
4. Para cada paquete, clonar su repositorio de AUR y copiar `PKGBUILD` y `.SRCINFO`:

   ```bash
   git clone ssh://aur@aur.archlinux.org/tlacuache-browser-git.git
   cp packaging/aur/tlacuache-browser-git/{PKGBUILD,.SRCINFO} tlacuache-browser-git/
   cd tlacuache-browser-git && git add PKGBUILD .SRCINFO && git commit -m "Versión inicial" && git push
   ```

   Requiere una cuenta de AUR con la llave SSH registrada.
5. En cada versión nueva: subir `pkgver` (y `version` en `Cargo.toml`), repetir los pasos 2–4 y reiniciar `pkgrel=1`.
