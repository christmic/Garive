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
        XCTAssertTrue(app.navigationBars["New remote task"].waitForExistence(timeout: 3))
        let synthesize = starterButton("Synthesize", in: app)
        XCTAssertTrue(synthesize.exists)
        XCTAssertTrue(starterButton("Analyze", in: app).exists)
        XCTAssertTrue(starterButton("Create", in: app).exists)
        synthesize.tap()
        XCTAssertTrue(app.buttons["Start on server"].isEnabled)
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
    private func walkthroughApp(_ extraArguments: String...) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--garive-walkthrough"] + extraArguments
        return app
    }

    @MainActor
    private func starterButton(_ label: String, in app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", label)).firstMatch
    }
}
