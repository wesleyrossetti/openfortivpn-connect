# OpenFortiVpn Connect

Aplicativo desktop para Linux/Fedora para conectar VPNs `openfortivpn` com interface gráfica.

## Suporte

- Fedora Linux
- outras distribuições Linux compatíveis com `pkexec`, `polkit` e `openfortivpn`

## Instalação no Fedora

### Pacotes de runtime

```bash
sudo dnf install openfortivpn polkit libayatana-appindicator-gtk3
```

### Pacotes de desenvolvimento

```bash
sudo dnf install \
  gcc gcc-c++ make pkg-config openssl-devel \
  webkit2gtk4.1-devel javascriptcoregtk4.1-devel \
  libsoup3-devel gtk3-devel libayatana-appindicator-gtk3-devel \
  rpm
```

## Executar a partir do código-fonte

```bash
npm install
npm run build:openfortivpn
cargo tauri dev
```

O binário `openfortivpn` pode ser usado de três formas:

- embutido no projeto em `src-tauri/openfortivpn/openfortivpn`
- em `/usr/local/bin/openfortivpn`
- em `/usr/bin/openfortivpn`

O app usa `pkexec` para iniciar o helper privilegiado, aplicar DNS e conectar/desconectar a VPN.

## Gerar RPM

```bash
npm install
npm run build:openfortivpn
cargo tauri build --bundles rpm
```

O RPM será gerado em:

```bash
src-tauri/target/release/bundle/rpm/
```

## Instalar o RPM

Depois de gerar o pacote:

```bash
sudo rpm -Uvh --replacepkgs --replacefiles "src-tauri/target/release/bundle/rpm/OpenFortiVpn Connect-0.1.8-1.x86_64.rpm"
```

## Release

As releases publicadas ficam em:

https://github.com/wesleyrossetti/openfortivpn-connect/releases

## Dependências do pacote

O RPM já declara a dependência runtime necessária para o indicador do sistema:

- `libayatana-appindicator-gtk3`

## Licença

MIT
