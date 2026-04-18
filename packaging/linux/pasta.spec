%global bundle_id    com.pasta.launcher
%global bin_name     pasta-launcher
# Upstream vendors ONNX Runtime via `ort` and bundles rusqlite; skip the
# Fedora debug-source extraction step that otherwise trips on those vendored
# C/C++ sources.
%global debug_package %{nil}

Name:           pasta
# Version line is rewritten by .github/workflows/release.yml for tagged builds;
# the 0.1.0 default is the local/mock-build version.
Version:        0.1.0
Release:        1%{?dist}
Summary:        Fast local-first clipboard launcher with search and secrets

License:        MIT
URL:            https://github.com/yafetgetachew/pasta
Source0:        %{name}-%{version}.tar.gz

ExclusiveArch:  x86_64 aarch64

BuildRequires:  rust >= 1.82
BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  pkgconfig
BuildRequires:  pkgconfig(xkbcommon)
BuildRequires:  pkgconfig(fontconfig)
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  pkgconfig(dbus-1)
BuildRequires:  pkgconfig(openssl)
BuildRequires:  pkgconfig(libsecret-1)
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib

Requires:       polkit
Requires:       libxkbcommon
Requires:       fontconfig
Requires:       libwayland-client
Requires:       dbus-libs
Requires:       libsecret

Recommends:     howdy
Recommends:     polkit-gnome
Suggests:       gnome-shell-extension-appindicator

%description
Pasta is a clipboard history launcher designed for speed and local-first
privacy. It supports full-text and semantic search, syntax-highlighted
previews, tagged collections, and encrypted "secrets" that are masked in
the UI until authenticated. On Linux, revealing a secret triggers a
polkit prompt backed by PAM, so password or Howdy face recognition work
out of the box.

%prep
%autosetup -n %{name}-%{version}

%build
# Keep debug symbols stripped via our Cargo.toml [profile.release] settings.
# `--locked` ensures reproducible builds from Cargo.lock.
cargo build --release --locked

%install
install -Dm0755 target/release/%{bin_name} %{buildroot}%{_bindir}/%{bin_name}
install -Dm0644 assets/pasta.png \
    %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/%{bundle_id}.png
install -Dm0644 packaging/linux/%{bundle_id}.desktop \
    %{buildroot}%{_datadir}/applications/%{bundle_id}.desktop
install -Dm0644 packaging/linux/%{bundle_id}.policy \
    %{buildroot}%{_datadir}/polkit-1/actions/%{bundle_id}.policy

desktop-file-validate %{buildroot}%{_datadir}/applications/%{bundle_id}.desktop

%files
%license LICENSE
%doc README.md
%{_bindir}/%{bin_name}
%{_datadir}/applications/%{bundle_id}.desktop
%{_datadir}/icons/hicolor/512x512/apps/%{bundle_id}.png
%{_datadir}/polkit-1/actions/%{bundle_id}.policy

%changelog
* Fri Apr 18 2026 Yafet Getachew <you@example.com> - 0.1.0-1
- Initial Fedora package
- Bundles polkit action for secret reveal and clear-history flows
