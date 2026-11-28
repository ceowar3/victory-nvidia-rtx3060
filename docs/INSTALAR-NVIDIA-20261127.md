# Instalar y ejecutar VIctory NVIDIA v2026.11.27.9

Publicación mantenida por ceowar3. Guía específica para esta versión; las otras ramas del repositorio son builds distintas.

## 1. Preparar el PC

- Windows 10 u 11 de 64 bits, SSD, al menos 32 GB de RAM y tarjeta NVIDIA GeForce RTX con un controlador reciente: requisitos indicados por esta versión, no una garantía de FPS.
- Necesitas tus propios archivos completos del juego Xbox Series, versión 1.0.2.1, incluido game_scarlett_master.exe. Aquí no se distribuye el juego.
- No necesitas Rust, Visual Studio ni descargar el código para utilizar el ejecutable publicado.
- Cierra cualquier instancia anterior de victory.exe. Conserva una copia de tus partidas antes de cambiar de versión. No mezcles ejecutables de diferentes builds en la misma carpeta.

## 2. Descargar

Publicación y archivos:
https://github.com/ceowar3/victory-nvidia-rtx3060/releases/tag/nvidia-v2026.11.27.9

Ejecutable listo para usar:
https://github.com/ceowar3/victory-nvidia-rtx3060/releases/download/nvidia-v2026.11.27.9/victory.exe

Código completo, opcional:
https://github.com/ceowar3/victory-nvidia-rtx3060/releases/download/nvidia-v2026.11.27.9/VIctory-NVIDIA-v2026.11.27.9-source.zip

SHA-256 esperado de victory.exe:
FC363D6DE7B5737AFB38E4C979A57912993A96E147A396704EA2BE43740B236B

Para comprobarlo en PowerShell (ajusta la ruta):

```powershell
Get-FileHash -LiteralPath "$env:USERPROFILE\Downloads\victory.exe" -Algorithm SHA256
```

## 3. Colocar y abrir

Copia victory.exe junto al ejecutable del juego. Ejemplo de estructura, conservando todos los demás archivos del juego:

```text
D:\Games\GTA-VI\
  game_scarlett_master.exe
  victory.exe
  ...los demás archivos completos del juego...
```

Haz doble clic en victory.exe. También puedes iniciarlo desde PowerShell:

```powershell
Set-Location 'D:\Games\GTA-VI'
.\victory.exe
```

El archivo no está firmado. Si Windows muestra un aviso, verifica primero el origen y el hash; no desactives la protección del sistema para instalarlo.

No uses Run.ps1 de main para esta descarga: ese lanzador corresponde a la integración paired. Esta versión tiene su propio arranque mediante victory.exe.

## 4. Primera ejecución y componentes opcionales

- El programa puede solicitar descargar componentes DLSS de NVIDIA. Si los rechazas, puede arrancar con el escalado del juego y sin esas funciones; no descargues DLL sueltas de páginas ajenas.
- Puede solicitar los componentes AMD FidelityFX necesarios para FSR 3. Son descargas separadas del ejecutable.
- En Windows 10 puede ofrecer descargar D3D12 Agility para los shaders que requieren Shader Model 6.6. La descarga se coloca en una carpeta D3D12 junto al ejecutable. Windows 11 utiliza normalmente su runtime del sistema.
- Esta release anuncia un reinicio automático único de calentamiento al estrenar versión, tras unos 10 segundos. No equivale a una mejora de FPS comprobada en tu PC.
- Las descargas requieren conexión y espacio libre. No se incluye una cifra universal de espacio para el juego y sus cachés.

## 5. Opciones dentro del programa

- F1: menú de ajustes; flechas para navegar/cambiar y Enter para aplicar.
- F3: estadísticas de FPS, GPU, memoria y generación de fotogramas.
- F1 > EXPORT SAVE: exporta una partida a un ZIP en la carpeta Saves del juego.
- F1 > IMPORT SAVE: elige un ZIP de Saves; la importación se aplica en el próximo inicio. Esta versión anuncia una copia de seguridad previa y mantiene los ajustes personales separados.
- RTX 20/30: no presupongas generación DLSS nativa de RTX 40/50. La versión ofrece opciones FSR 3 sujetas a sus dependencias. Los FPS generados no demuestran que la simulación vaya a esa velocidad.

## 6. Dónde quedan los datos

Ubicación predeterminada: %LOCALAPPDATA%\victory. Allí se guardan ajustes, partidas, cachés, logs y componentes descargados; la carpeta D3D12 descrita arriba es una excepción.

Para usar otra unidad, antes de iniciar y con el programa cerrado, abre PowerShell y ejecuta:

```powershell
[Environment]::SetEnvironmentVariable('VICTORY_HOME', 'D:\VIctoryData', 'User')
```

Abre una ventana nueva antes de arrancar. Si ya tienes datos, respáldalos y transfiérelos a la nueva ubicación con el programa cerrado; cambiar la variable no migra los archivos. No borres carpetas antiguas hasta verificar las partidas.

## 7. Si se cierra, se ve negro o va lento

No está garantizado que esta release corrija las cinemáticas negras ni los FPS bajos. No borres partidas ni todas las cachés como primer paso. Anota versión exacta, GPU, controlador, RAM, Windows, escena y ajustes.

Recoge los dos tipos de log del intento fallido:

- Log de ejecución: %LOCALAPPDATA%\victory\logs\, el archivo victory-nvidia-play-*.log más reciente. La ventana también muestra la ruta en la línea Log:.
- Log de cierre: %LOCALAPPDATA%\VIctory\data\7952987A\crashes.log y, si existe, el archivo victory-*.crash.log reciente de data\7952987A\temp\.

Si configuraste VICTORY_HOME, busca estos datos bajo esa carpeta. Revisa los logs antes de compartirlos para retirar rutas personales u otros datos privados.

## 8. Compilar tú mismo: opcional

Instala Git, Rust mediante rustup y Visual Studio Build Tools con Desktop development with C++, MSVC x64 y Windows SDK. El archivo rust-toolchain.toml fija Rust 1.98.1.

```powershell
git clone --branch trustno1-v2026.11.27.9 --single-branch https://github.com/ceowar3/victory-nvidia-rtx3060.git VIctory-NVIDIA
Set-Location VIctory-NVIDIA
git rev-parse HEAD
cargo build --locked --release -p victory
```

El commit esperado es a4d05e7fd0af904130809c334a6008d62c24db6a. Tras compilar, copia target\release\victory.exe junto a game_scarlett_master.exe. Una compilación propia puede tener un hash distinto del binario descargado; no afirmes que son idénticos sin compararlos.

Licencias y avisos originales permanecen en el código. Esta guía no significa que se hayan verificado los FPS o el funcionamiento del juego en todos los equipos.