import Foundation

precondition(CommandLine.arguments.count >= 2, "usage: InputSourceNameTests.swift <tacit.app>")
guard let bundle = Bundle(path: CommandLine.arguments[1]),
      let inputModes = bundle.object(forInfoDictionaryKey: "ComponentInputModeDict") as? [String: Any],
      let visibleModes = inputModes["tsVisibleInputModeOrderedArrayKey"] as? [String],
      !visibleModes.isEmpty else {
    fatalError("Missing visible input-mode metadata in packaged bundle")
}

for modeID in visibleModes {
    precondition(bundle.localizedString(forKey: modeID, value: nil, table: "InfoPlist") == "Tacit",
                 "The packaged input mode must display Tacit, not its identifier")
}
print("Packaged input-source display name passed")
