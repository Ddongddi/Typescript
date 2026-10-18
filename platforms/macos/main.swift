// tacit for macOS: an InputMethodKit input method. All the typing logic is in
// libtacit_ffi (tacit.h); this maps key events to it and its commands to the client.
import Cocoa
import InputMethodKit

let connectionName = Bundle.main.object(forInfoDictionaryKey: "InputMethodConnectionName") as! String
let server = IMKServer(name: connectionName, bundleIdentifier: Bundle.main.bundleIdentifier)
// The one input method of the process; nil (keys pass through as typed) if it can't start.
let ime: OpaquePointer? = ts_new(nil)
_ = server
NSApplication.shared.run()
