import XCTest

final class GariveIOSUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    @MainActor
    func testRemoteNavigationAndNewTaskControlsAreReachable() throws {
        let app = walkthroughApp()
        app.launch()

        XCTAssertTrue(app.navigationBars["Remote"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.staticTexts["Connected · server work continues"].exists)
        app.buttons["Open navigation"].tap()
        XCTAssertTrue(app.buttons["Sessions"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.buttons["Agents"].exists)
        XCTAssertTrue(app.buttons["Settings"].exists)

        app.buttons["Sessions"].tap()
        XCTAssertTrue(app.navigationBars["Sessions"].waitForExistence(timeout: 3))
        app.buttons["New task"].tap()
        XCTAssertTrue(
            app.navigationBars["New remote task"].waitForExistence(timeout: 2)
                || app.navigationBars["New task"].waitForExistence(timeout: 2)
        )
        let form = app.collectionViews.firstMatch
        let synthesize = starterButton("Synthesize", in: app)
        XCTAssertTrue(reveal(synthesize, in: form))
        XCTAssertTrue(starterButton("Analyze", in: app).exists)
        XCTAssertTrue(starterButton("Create", in: app).exists)
        synthesize.tap()
        XCTAssertTrue(app.buttons["Start on server"].isEnabled)
        app.buttons["Start on server"].tap()
        XCTAssertTrue(app.navigationBars["Mobile Orchestrator"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.staticTexts["Turn notes into a clear decision memo"].exists)

        app.buttons["Stop current work"].tap()
        app.buttons["Stop turn"].tap()
        XCTAssertTrue(app.staticTexts["Cancellation recorded. Committed work remains available."].waitForExistence(timeout: 8))
        let composer = app.textFields["Give the Agent direction"]
        XCTAssertTrue(composer.waitForExistence(timeout: 3))
        composer.tap()
        composer.typeText("Prepare the final mobile handoff")
        app.buttons["Send to Agent"].tap()
        XCTAssertTrue(app.staticTexts["Prepare the final mobile handoff"].waitForExistence(timeout: 8))
    }

    @MainActor
    func testConversationExposesBoundedDecisionAndCancelControls() throws {
        let app = walkthroughApp("--garive-walkthrough-conversation")
        app.launch()

        XCTAssertTrue(app.buttons["Approve once"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.buttons["Decline"].exists)
        XCTAssertTrue(app.buttons["Activity · 1"].exists)
        app.buttons["Activity · 1"].tap()
        XCTAssertTrue(app.staticTexts["Verification"].waitForExistence(timeout: 2))

        app.buttons["Stop current work"].tap()
        XCTAssertTrue(app.buttons["Stop turn"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.staticTexts["Committed work remains in the timeline."].exists)
    }

    @MainActor
    func testPairingRequiresSecureExplicitFields() throws {
        let app = XCUIApplication()
        app.launch()

        XCTAssertTrue(app.staticTexts["Your agents, wherever you are"].waitForExistence(timeout: 5))
        let service = app.textFields["Service address"]
        let code = app.secureTextFields["One-time access code"]
        let connect = app.buttons["Connect securely"]
        XCTAssertTrue(service.exists)
        XCTAssertTrue(code.exists)
        XCTAssertFalse(connect.isEnabled)
        service.tap()
        service.typeText("https://agent.example.test")
        code.tap()
        code.typeText("fresh-code")
        XCTAssertTrue(connect.isEnabled)
    }

    @MainActor
    func testSettingsDiagnosticsAndUnpairRemainExplicit() throws {
        let app = walkthroughApp("--garive-walkthrough-settings")
        app.launch()

        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.staticTexts["Access grant protected by Keychain"].exists)
        let list = app.collectionViews.firstMatch
        let light = app.buttons["Light"]
        XCTAssertTrue(reveal(light, in: list))
        light.tap()
        XCTAssertTrue(light.isSelected)
        let dark = app.buttons["Dark"]
        dark.tap()
        XCTAssertTrue(dark.isSelected)
        app.buttons["System"].tap()
        XCTAssertTrue(app.buttons["System"].isSelected)
        XCTAssertTrue(reveal(app.buttons["Open notification settings"], in: list))
        let diagnostics = app.buttons["Copy safe diagnostics"]
        XCTAssertTrue(reveal(diagnostics, in: list))
        diagnostics.tap()
        XCTAssertTrue(app.buttons["Diagnostics copied"].exists)
        let unpair = app.buttons["Unpair this device"]
        XCTAssertTrue(reveal(unpair, in: list))
        unpair.tap()
        XCTAssertTrue(app.buttons["Unpair device"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.staticTexts["This removes access from this phone. Agent work and history remain on your service."].exists)
    }

    @MainActor
    private func walkthroughApp(_ extraArguments: String...) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--garive-walkthrough"] + extraArguments
        return app
    }

    @MainActor
    private func starterButton(_ label: String, in app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", label)).firstMatch
    }

    @MainActor
    private func reveal(_ element: XCUIElement, in scrollView: XCUIElement) -> Bool {
        for _ in 0..<5 {
            if element.exists && element.isHittable { return true }
            scrollView.swipeUp()
        }
        return element.exists && element.isHittable
    }
}
