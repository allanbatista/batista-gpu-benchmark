PREFIX ?= /usr/local
DESTDIR ?=
BIN := target/release/batista-gpu-benchmark
SHARE := $(DESTDIR)$(PREFIX)/share

.PHONY: build test install uninstall deb rpm appimage snap flatpak clean

build:
	cargo build --release

test:
	cargo test --release

install: build
	install -Dm755 $(BIN) $(DESTDIR)$(PREFIX)/bin/batista-gpu-benchmark
	mkdir -p $(SHARE)/batista-gpu-benchmark
	cp -r assets $(SHARE)/batista-gpu-benchmark/
	install -Dm644 packaging/batista-gpu-benchmark.desktop $(SHARE)/applications/batista-gpu-benchmark.desktop
	install -Dm644 packaging/icon.png $(SHARE)/icons/hicolor/256x256/apps/batista-gpu-benchmark.png

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/batista-gpu-benchmark
	rm -rf $(SHARE)/batista-gpu-benchmark
	rm -f $(SHARE)/applications/batista-gpu-benchmark.desktop
	rm -f $(SHARE)/icons/hicolor/256x256/apps/batista-gpu-benchmark.png

# Debian/Ubuntu package (needs: cargo install cargo-deb)
deb: build
	cargo deb --no-build

# RPM package for dnf/yum distros (needs: cargo install cargo-generate-rpm)
rpm: build
	cargo generate-rpm

appimage: build
	bash packaging/build-appimage.sh

# Stages the prebuilt binary + assets, then packs with snapcraft (dump plugin).
snap: build
	rm -rf snap-stage
	mkdir -p snap-stage/usr/bin snap-stage/usr/share/batista-gpu-benchmark
	install -m755 $(BIN) snap-stage/usr/bin/
	cp -r assets snap-stage/usr/share/batista-gpu-benchmark/
	snapcraft pack

# Stages the prebuilt binary + assets, then builds a single-file .flatpak bundle.
flatpak: build
	rm -rf pkg-root flatpak-repo flatpak-build
	mkdir -p pkg-root
	install -m755 $(BIN) pkg-root/
	cp -r assets pkg-root/
	cp packaging/batista-gpu-benchmark.desktop packaging/icon.png pkg-root/
	flatpak-builder --user --force-clean --repo=flatpak-repo flatpak-build \
		packaging/flatpak/com.allanbatista.GpuBenchmark.yml
	flatpak build-bundle flatpak-repo batista-gpu-benchmark.flatpak com.allanbatista.GpuBenchmark
	@echo "OK: batista-gpu-benchmark.flatpak"

clean:
	cargo clean
	rm -rf snap-stage pkg-root flatpak-repo flatpak-build *.snap *.flatpak
