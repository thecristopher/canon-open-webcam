import Testing
@testable import SetupKit

@Test func theCreditNamesItsAuthor() {
    #expect(Credits.madeBy == "Made by Cristopher Cervantes")
}

@Test func theDisclaimerSaysCanonHadNothingToDoWithIt() {
    #expect(Credits.disclaimer.contains("not affiliated with or endorsed by Canon"))
}
