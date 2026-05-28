## How to build an arm64 windows binary for testing:

Getting a good crash dump (plus accompanying binaries) for arm64 Windows is a bit of a pain if you aren't developing on Windows; here is the best way I've found.

- Get a Windows vm (VirtualBox works fine) + Windows 11 image (you can download this from Microsoft directly)
	- You'll probably need to install whatever tooling the VM needs for good display/file transfer support.  For VirtualBox (on macOS), you want the VM to be running, and the VM's window focused.  Select "devices", then select "insert guest additions CD", which will do the install, and then you won't need a microscope to use the VM.

- Install Visual Studio (_not_ Code, the full thing.)  The free version will suffice.  You can be very frugal with install options, just make sure all the native/C++ stuff is there.
	- This will install MSVC, CMake--everything you should need for building.

- Install Git for windows

- Install Sentry CLI

- Clone sentry native sdk (https://github.com/getsentry/sentry-native)

- Build the sentry crashing example.  Do this from the prompt, using the "ARM64 Native Tools Command Line Prompt" shortcut.  First, do a build and install (as outlined in "Building and Installation" on the readme.)  Then, build `sentry_example` (this should just be a cmake target, invoked like `cmake --build sentry_example` )

At this point, you should have a working sentry_example that can crash, oom, emit events/logs, and more.  You'll want to point it at your sentry instance if you want to actually go through the whole ingestion pipeline.  This means you'll want to bind your sentry instance to your hardware address (just use `0.0.0.0`) instead of `localhost`, so that you can reach it from within the VM.
