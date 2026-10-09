# Everything you need, from a fresh clone to "it starts at login"

.PHONY: build test install uninstall doctor

build:
	swift build -c release

test:
	swift test

install: build
	.build/release/canon-webcam install

uninstall:
	.build/release/canon-webcam uninstall

doctor: build
	.build/release/canon-webcam doctor
