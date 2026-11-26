Name:           filegram
Version:        __VERSION__
Release:        1%{?dist}
Summary:        Disk space analyzer with an interactive treemap

License:        MIT
URL:            https://github.com/filegram/filegram-desktop
Source0:        https://github.com/filegram/filegram-desktop/archive/refs/tags/v%{version}.tar.gz#/%{name}-%{version}.tar.gz

BuildRequires:  rust-packaging
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  libxkbcommon-devel
BuildRequires:  wayland-devel
BuildRequires:  desktop-file-utils

%description
Filegram scans a directory tree and visualizes disk usage as an interactive
treemap, helping to find what takes up space.

%prep
%autosetup -n filegram-desktop-%{version}
if ! grep -q '^\[profile.rpm\]' Cargo.toml; then
    printf '\n[profile.rpm]\ninherits = "release"\n' >> Cargo.toml
fi

%build
%cargo_build

%install
%cargo_install
install -Dm644 packaging/linux/filegram.desktop %{buildroot}%{_datadir}/applications/filegram.desktop
install -Dm644 assets/icon/icon.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/filegram.svg

%check
%cargo_test
desktop-file-validate %{buildroot}%{_datadir}/applications/filegram.desktop

%files
%license LICENSE
%{_bindir}/filegram
%{_datadir}/applications/filegram.desktop
%{_datadir}/icons/hicolor/scalable/apps/filegram.svg

%changelog
* Thu Nov 26 2026 stan220 <stas@aliferovich.com> - 0.2.12-1
- Initial COPR package
