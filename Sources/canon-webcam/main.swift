import Foundation
import WebcamEngine

setvbuf(stdout, nil, _IOLBF, 0)

let exitCode: Int32
switch CLICommand.parse(Array(CommandLine.arguments.dropFirst())) {
case .failure(let error):
    printError(error.description)
    exitCode = 2
case .success(let command):
    switch command {
    case .start(let alwaysOn): exitCode = startEngine(alwaysOn: alwaysOn)
    case .status: exitCode = showStatus()
    case .set(let name, let value): exitCode = sendToEngine(.set(name, to: value))
    case .focus(let action, let steps): exitCode = sendToEngine(.focus(action, steps: steps))
    case .doctor: exitCode = runDoctor()
    case .install(let alwaysOn): exitCode = installAgent(alwaysOn: alwaysOn)
    case .uninstall(let restoreCanon): exitCode = uninstallAgent(restoreCanon: restoreCanon)
    case .bench(let seconds): exitCode = runBenchmark(seconds: seconds)
    case .help:
        print(CLICommand.usage)
        exitCode = 0
    }
}
exit(exitCode)
