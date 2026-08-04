import XCTest
@testable import FeatherKeyKit

final class DeviceKeyTests: XCTestCase {
    func test_loadOrCreate_is_32_bytes_and_stable() throws {
        let acct = "test-\(UUID().uuidString)"
        defer { try? DeviceKey.delete(account: acct) }
        let a = try DeviceKey.loadOrCreate(account: acct)
        let b = try DeviceKey.loadOrCreate(account: acct)
        XCTAssertEqual(a.count, 32)
        XCTAssertEqual(a, b)   // second call returns the same stored key
    }
}
