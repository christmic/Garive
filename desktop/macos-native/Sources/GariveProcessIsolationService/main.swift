import Darwin
import Foundation
import GariveProcessService

do {
    let bootstrap = try ProcessServiceBootstrapConfiguration.current(bundle: .main)
    let endpoint = try ProcessServiceEndpoint.validated()
    let delegate = ProcessServiceListenerDelegate(
        admissionPolicy: bootstrap.admissionPolicy,
        endpoint: endpoint
    )
    let listener = NSXPCListener.service()
    listener.delegate = delegate
    bootstrap.admissionPolicy.configure(listener)
    listener.activate()
    dispatchMain()
} catch {
    exit(EXIT_FAILURE)
}
