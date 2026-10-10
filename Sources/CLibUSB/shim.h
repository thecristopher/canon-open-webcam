// Homebrew puts libusb in different places on Apple Silicon and Intel
#if __has_include("/opt/homebrew/opt/libusb/include/libusb-1.0/libusb.h")
#include "/opt/homebrew/opt/libusb/include/libusb-1.0/libusb.h"
#else
#include "/usr/local/opt/libusb/include/libusb-1.0/libusb.h"
#endif
