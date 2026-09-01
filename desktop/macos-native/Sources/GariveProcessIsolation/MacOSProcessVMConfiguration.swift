import CryptoKit
import Foundation
import Virtualization

/// The only workspace authority projected into a V0-A process VM.
public enum MacOSProcessWorkspaceModeV1: UInt8, Sendable {
    case readOnly = 0
    case readWrite = 1
}

/// Closed, path-free failures admitted by the V0-A configuration boundary.
public enum MacOSProcessVMPlanError: String, Error, Equatable, Sendable {
    case invalidURL = "invalid_url"
    case invalidDigest = "invalid_digest"
    case invalidProtocolRevision = "invalid_protocol_revision"
    case invalidCPUCount = "invalid_cpu_count"
    case invalidMemorySize = "invalid_memory_size"
    case invalidControlTimeout = "invalid_control_timeout"
    case resourceUnavailable = "resource_unavailable"
}

/// Immutable, fully explicit inputs for one macOS-native process VM.
public struct MacOSProcessVMPlanV1: Sendable {
    fileprivate let kernelURL: URL
    fileprivate let kernelDigest: String
    fileprivate let initialRamdiskURL: URL
    fileprivate let initialRamdiskDigest: String
    fileprivate let rootDiskURL: URL
    fileprivate let rootDiskDigest: String
    fileprivate let workspaceURL: URL
    fileprivate let workspaceMode: MacOSProcessWorkspaceModeV1
    fileprivate let guestProtocolRevision: String
    fileprivate let cpuCount: Int
    fileprivate let memorySizeBytes: UInt64
    fileprivate let controlTimeoutMilliseconds: UInt64

    public let bindingDigest: String

    public init(
        kernelURL: URL,
        kernelDigest: String,
        initialRamdiskURL: URL,
        initialRamdiskDigest: String,
        rootDiskURL: URL,
        rootDiskDigest: String,
        workspaceURL: URL,
        workspaceMode: MacOSProcessWorkspaceModeV1,
        guestProtocolRevision: String,
        cpuCount: Int,
        memorySizeBytes: UInt64,
        controlTimeoutMilliseconds: UInt64
    ) throws {
        let urls = [kernelURL, initialRamdiskURL, rootDiskURL, workspaceURL]
        guard urls.allSatisfy(Self.isAbsoluteLocalFileURL) else {
            throw MacOSProcessVMPlanError.invalidURL
        }
        let digests = [kernelDigest, initialRamdiskDigest, rootDiskDigest]
        guard digests.allSatisfy(Self.isCanonicalSHA256) else {
            throw MacOSProcessVMPlanError.invalidDigest
        }
        guard Self.isSafeProtocolRevision(guestProtocolRevision) else {
            throw MacOSProcessVMPlanError.invalidProtocolRevision
        }
        guard cpuCount >= VZVirtualMachineConfiguration.minimumAllowedCPUCount,
              cpuCount <= VZVirtualMachineConfiguration.maximumAllowedCPUCount
        else { throw MacOSProcessVMPlanError.invalidCPUCount }
        guard memorySizeBytes >= VZVirtualMachineConfiguration.minimumAllowedMemorySize,
              memorySizeBytes <= VZVirtualMachineConfiguration.maximumAllowedMemorySize,
              memorySizeBytes.isMultiple(of: 1_048_576)
        else { throw MacOSProcessVMPlanError.invalidMemorySize }
        guard (1 ... 60_000).contains(controlTimeoutMilliseconds) else {
            throw MacOSProcessVMPlanError.invalidControlTimeout
        }

        self.kernelURL = kernelURL
        self.kernelDigest = kernelDigest
        self.initialRamdiskURL = initialRamdiskURL
        self.initialRamdiskDigest = initialRamdiskDigest
        self.rootDiskURL = rootDiskURL
        self.rootDiskDigest = rootDiskDigest
        self.workspaceURL = workspaceURL
        self.workspaceMode = workspaceMode
        self.guestProtocolRevision = guestProtocolRevision
        self.cpuCount = cpuCount
        self.memorySizeBytes = memorySizeBytes
        self.controlTimeoutMilliseconds = controlTimeoutMilliseconds
        bindingDigest = Self.digest(
            kernelURL, kernelDigest, initialRamdiskURL, initialRamdiskDigest,
            rootDiskURL, rootDiskDigest, workspaceURL, workspaceMode,
            guestProtocolRevision, cpuCount, memorySizeBytes,
            controlTimeoutMilliseconds
        )
    }

    private static func isAbsoluteLocalFileURL(_ url: URL) -> Bool {
        url.isFileURL && url.path.hasPrefix("/") && url.query == nil && url.fragment == nil
            && (url.host == nil || url.host == "" || url.host == "localhost")
    }

    private static func isCanonicalSHA256(_ value: String) -> Bool {
        value.utf8.count == 64 && value.utf8.allSatisfy {
            (48 ... 57).contains($0) || (97 ... 102).contains($0)
        }
    }

    private static func isSafeProtocolRevision(_ value: String) -> Bool {
        let bytes = Array(value.utf8)
        guard (1 ... 128).contains(bytes.count),
              let first = bytes.first, let last = bytes.last,
              isASCIIAlphanumeric(first), isASCIIAlphanumeric(last)
        else { return false }
        return bytes.allSatisfy {
            isASCIIAlphanumeric($0) || $0 == 45 || $0 == 46
        }
    }

    private static func isASCIIAlphanumeric(_ byte: UInt8) -> Bool {
        (48 ... 57).contains(byte) || (97 ... 122).contains(byte)
    }

    private static func digest(
        _ kernelURL: URL, _ kernelDigest: String,
        _ ramdiskURL: URL, _ ramdiskDigest: String,
        _ rootURL: URL, _ rootDigest: String,
        _ workspaceURL: URL, _ mode: MacOSProcessWorkspaceModeV1,
        _ revision: String, _ cpu: Int, _ memory: UInt64, _ timeout: UInt64
    ) -> String {
        let commandLine = MacOSProcessVMConfigurationV1.kernelCommandLine(revision: revision)
        var data = Data("garive.macos-process-vm-config.v1".utf8)
        for value in [
            kernelURL.absoluteString, kernelDigest, ramdiskURL.absoluteString,
            ramdiskDigest, rootURL.absoluteString, rootDigest,
            workspaceURL.absoluteString,
        ] { data.appendLengthDelimited(Data(value.utf8)) }
        data.append(mode.rawValue)
        data.appendLengthDelimited(Data(revision.utf8))
        data.appendBigEndian(UInt64(cpu))
        data.appendBigEndian(memory)
        data.appendBigEndian(timeout)
        data.appendLengthDelimited(Data(commandLine.utf8))
        return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

/// SDK objects retained together so tests and callers can prove attachment authority.
public struct BuiltMacOSProcessVMConfigurationV1 {
    let configuration: VZVirtualMachineConfiguration
    let rootDiskAttachment: VZDiskImageStorageDeviceAttachment
}

/// Pure V0-A projection. It cannot create or start a virtual machine.
public enum MacOSProcessVMConfigurationV1 {
    public static let workspaceTag = "garive-workspace"

    fileprivate static func kernelCommandLine(revision: String) -> String {
        "console=hvc0 panic=-1 ro root=/dev/vda garive.workspace_tag=\(workspaceTag) garive.protocol=\(revision)"
    }

    public static func build(
        _ plan: MacOSProcessVMPlanV1
    ) throws -> BuiltMacOSProcessVMConfigurationV1 {
        let rootAttachment: VZDiskImageStorageDeviceAttachment
        do {
            rootAttachment = try VZDiskImageStorageDeviceAttachment(
                url: plan.rootDiskURL,
                readOnly: true
            )
        } catch {
            throw MacOSProcessVMPlanError.resourceUnavailable
        }

        let bootLoader = VZLinuxBootLoader(kernelURL: plan.kernelURL)
        bootLoader.initialRamdiskURL = plan.initialRamdiskURL
        bootLoader.commandLine = kernelCommandLine(revision: plan.guestProtocolRevision)

        let directory = VZSharedDirectory(
            url: plan.workspaceURL,
            readOnly: plan.workspaceMode == .readOnly
        )
        let fileSystem = VZVirtioFileSystemDeviceConfiguration(tag: workspaceTag)
        fileSystem.share = VZSingleDirectoryShare(directory: directory)

        let configuration = VZVirtualMachineConfiguration()
        configuration.platform = VZGenericPlatformConfiguration()
        configuration.bootLoader = bootLoader
        configuration.cpuCount = plan.cpuCount
        configuration.memorySize = plan.memorySizeBytes
        configuration.storageDevices = [VZVirtioBlockDeviceConfiguration(attachment: rootAttachment)]
        configuration.directorySharingDevices = [fileSystem]
        configuration.socketDevices = [VZVirtioSocketDeviceConfiguration()]
        configuration.entropyDevices = [VZVirtioEntropyDeviceConfiguration()]
        configuration.networkDevices = []
        configuration.audioDevices = []
        configuration.consoleDevices = []
        configuration.memoryBalloonDevices = []
        configuration.serialPorts = []
        configuration.keyboards = []
        configuration.pointingDevices = []
        configuration.graphicsDevices = []
        if #available(macOS 15.0, *) { configuration.usbControllers = [] }

        return BuiltMacOSProcessVMConfigurationV1(
            configuration: configuration,
            rootDiskAttachment: rootAttachment
        )
    }
}

private extension Data {
    mutating func appendLengthDelimited(_ value: Data) {
        appendBigEndian(UInt64(value.count))
        append(value)
    }

    mutating func appendBigEndian(_ value: UInt64) {
        var encoded = value.bigEndian
        Swift.withUnsafeBytes(of: &encoded) { append(contentsOf: $0) }
    }
}
