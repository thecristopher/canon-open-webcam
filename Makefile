# Everything you need, from a fresh clone to "it starts at login"

.PHONY: build test dist install uninstall doctor

build:
	swift build -c release

test:
	swift test

# The friendly way: a disk image with "Install Canon Webcam" and "Uninstall Canon Webcam"
dist:
	./scripts/build-apps.sh

install: build
	.build/release/canon-webcam install

uninstall:
	.build/release/canon-webcam uninstall

doctor: build
	.build/release/canon-webcam doctor
