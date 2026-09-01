import Foundation
import Virtualization
import XCTest
@testable import GariveProcessIsolation

final class MacOSProcessVMConfigurationTests: XCTestCase {
    private struct Inputs {
        var kernel: URL
        var kernelDigest = String(repeating: "1", count: 64)
        var ramdisk: URL
        var ramdiskDigest = String(repeating: "2", count: 64)
        var rootDisk: URL
        var rootDigest = String(repeating: "3", count: 64)
        var workspace: URL
        var mode = MacOSProcessWorkspaceModeV1.readOnly
        var revision = "guest-v1.0"
        var cpu = VZVirtualMachineConfiguration.minimumAllowedCPUCount
        var memory = VZVirtualMachineConfiguration.minimumAllowedMemorySize
        var timeout: UInt64 = 5_000

        func plan() throws -> MacOSProcessVMPlanV1 {
            try .init(
                kernelURL: kernel, kernelDigest: kernelDigest,
                initialRamdiskURL: ramdisk, initialRamdiskDigest: ramdiskDigest,
                rootDiskURL: rootDisk, rootDiskDigest: rootDigest,
                workspaceURL: workspace, workspaceMode: mode,
                guestProtocolRevision: revision, cpuCount: cpu,
                memorySizeBytes: memory, controlTimeoutMilliseconds: timeout
            )
        }
    }

    private func fixture() throws -> Inputs {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        for name in ["kernel", "initrd", "root.raw"] {
            XCTAssertTrue(FileManager.default.createFile(atPath: root.appendingPathComponent(name).path, contents: Data()))
        }
        return Inputs(
            kernel: root.appendingPathComponent("kernel"),
            ramdisk: root.appendingPathComponent("initrd"),
            rootDisk: root.appendingPathComponent("root.raw"),
            workspace: root
        )
    }

    func testDeterministicExactConfiguration() throws {
        let input = try fixture()
        let first = try input.plan()
        let second = try input.plan()
        XCTAssertEqual(first.bindingDigest, second.bindingDigest)

        let built = try MacOSProcessVMConfigurationV1.build(first)
        let config = built.configuration
        XCTAssertTrue(config.platform is VZGenericPlatformConfiguration)
        let boot = try XCTUnwrap(config.bootLoader as? VZLinuxBootLoader)
        XCTAssertEqual(boot.kernelURL, input.kernel)
        XCTAssertEqual(boot.initialRamdiskURL, input.ramdisk)
        XCTAssertEqual(config.cpuCount, input.cpu)
        XCTAssertEqual(config.memorySize, input.memory)
        XCTAssertEqual(config.storageDevices.count, 1)
        XCTAssertTrue(built.rootDiskAttachment.isReadOnly)
        XCTAssertEqual(config.directorySharingDevices.count, 1)
        let fileSystem = try XCTUnwrap(config.directorySharingDevices.first as? VZVirtioFileSystemDeviceConfiguration)
        XCTAssertEqual(fileSystem.tag, MacOSProcessVMConfigurationV1.workspaceTag)
        let share = try XCTUnwrap(fileSystem.share as? VZSingleDirectoryShare)
        XCTAssertTrue(share.directory.isReadOnly)
        XCTAssertEqual(config.socketDevices.count, 1)
        XCTAssertEqual(config.entropyDevices.count, 1)
        XCTAssertTrue(config.networkDevices.isEmpty)
        XCTAssertTrue(config.audioDevices.isEmpty)
        XCTAssertTrue(config.consoleDevices.isEmpty)
        XCTAssertTrue(config.memoryBalloonDevices.isEmpty)
        XCTAssertTrue(config.serialPorts.isEmpty)
        XCTAssertTrue(config.keyboards.isEmpty)
        XCTAssertTrue(config.pointingDevices.isEmpty)
        XCTAssertTrue(config.graphicsDevices.isEmpty)
        if #available(macOS 15.0, *) { XCTAssertTrue(config.usbControllers.isEmpty) }
    }

    func testBindingDigestHasStableCrossLanguageVector() throws {
        let plan = try MacOSProcessVMPlanV1(
            kernelURL: URL(fileURLWithPath: "/kernel"), kernelDigest: String(repeating: "1", count: 64),
            initialRamdiskURL: URL(fileURLWithPath: "/initrd"), initialRamdiskDigest: String(repeating: "2", count: 64),
            rootDiskURL: URL(fileURLWithPath: "/root.raw"), rootDiskDigest: String(repeating: "3", count: 64),
            workspaceURL: URL(fileURLWithPath: "/workspace", isDirectory: true), workspaceMode: .readOnly,
            guestProtocolRevision: "guest-v1.0", cpuCount: 1,
            memorySizeBytes: 4_194_304, controlTimeoutMilliseconds: 5_000
        )
        XCTAssertEqual(plan.bindingDigest, "af16266b6bb1f79bed52899ac7ee6cb7a2f545e0b820b712265986d629afbb85")
    }

    func testEveryInputFieldChangesBindingDigest() throws {
        let base = try fixture()
        let expected = try base.plan().bindingDigest
        var variants: [Inputs] = []
        func add(_ change: (inout Inputs) -> Void) { var value = base; change(&value); variants.append(value) }
        add { $0.kernel = $0.kernel.deletingLastPathComponent().appendingPathComponent("kernel-2") }
        add { $0.kernelDigest = String(repeating: "a", count: 64) }
        add { $0.ramdisk = $0.ramdisk.deletingLastPathComponent().appendingPathComponent("initrd-2") }
        add { $0.ramdiskDigest = String(repeating: "b", count: 64) }
        add { $0.rootDisk = $0.rootDisk.deletingLastPathComponent().appendingPathComponent("root-2.raw") }
        add { $0.rootDigest = String(repeating: "c", count: 64) }
        add { $0.workspace = $0.workspace.appendingPathComponent("other") }
        add { $0.mode = .readWrite }
        add { $0.revision = "guest-v1.1" }
        add { $0.cpu += 1 }
        add { $0.memory += 1_048_576 }
        add { $0.timeout += 1 }
        for variant in variants { XCTAssertNotEqual(try variant.plan().bindingDigest, expected) }
    }

    func testWriteModeChangesOnlyShareProjection() throws {
        var input = try fixture()
        input.mode = .readWrite
        let built = try MacOSProcessVMConfigurationV1.build(input.plan())
        let fileSystem = try XCTUnwrap(built.configuration.directorySharingDevices.first as? VZVirtioFileSystemDeviceConfiguration)
        let share = try XCTUnwrap(fileSystem.share as? VZSingleDirectoryShare)
        XCTAssertFalse(share.directory.isReadOnly)
        XCTAssertTrue(built.rootDiskAttachment.isReadOnly)
        XCTAssertTrue(built.configuration.networkDevices.isEmpty)
    }

    func testInvalidInputsFailClosed() throws {
        var input = try fixture()
        input.kernel = URL(string: "https://example.invalid/kernel")!
        XCTAssertThrowsError(try input.plan()) { XCTAssertEqual($0 as? MacOSProcessVMPlanError, .invalidURL) }
        input = try fixture(); input.rootDigest = String(repeating: "A", count: 64)
        XCTAssertThrowsError(try input.plan()) { XCTAssertEqual($0 as? MacOSProcessVMPlanError, .invalidDigest) }
        input = try fixture(); input.revision = "guest v1"
        XCTAssertThrowsError(try input.plan()) { XCTAssertEqual($0 as? MacOSProcessVMPlanError, .invalidProtocolRevision) }
        input = try fixture(); input.cpu = 0
        XCTAssertThrowsError(try input.plan()) { XCTAssertEqual($0 as? MacOSProcessVMPlanError, .invalidCPUCount) }
        input = try fixture(); input.memory += 1
        XCTAssertThrowsError(try input.plan()) { XCTAssertEqual($0 as? MacOSProcessVMPlanError, .invalidMemorySize) }
        input = try fixture(); input.timeout = 60_001
        XCTAssertThrowsError(try input.plan()) { XCTAssertEqual($0 as? MacOSProcessVMPlanError, .invalidControlTimeout) }
        input = try fixture(); try FileManager.default.removeItem(at: input.rootDisk)
        XCTAssertThrowsError(try MacOSProcessVMConfigurationV1.build(input.plan())) {
            XCTAssertEqual($0 as? MacOSProcessVMPlanError, .resourceUnavailable)
        }
    }

    func testTargetContainsNoForbiddenExecutionOrConfigurationSource() throws {
        let package = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        let source = package.appendingPathComponent("Sources/GariveProcessIsolation")
        let entries = FileManager.default.enumerator(at: source, includingPropertiesForKeys: nil)
        let files = (entries?.allObjects as? [URL]) ?? []
        let text = try files.filter { $0.pathExtension == "swift" }
            .map { try String(contentsOf: $0, encoding: .utf8) }.joined()
        for forbidden in ["ProcessInfo.processInfo.environment", "getenv(", "sandbox-exec", "sandbox_init", "Process()", "VZVirtualMachine(", ".start("] {
            XCTAssertFalse(text.contains(forbidden), "forbidden source: \(forbidden)")
        }
    }
}
