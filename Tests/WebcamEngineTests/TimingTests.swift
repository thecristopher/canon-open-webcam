import Foundation
import Testing
@testable import WebcamEngine

private let noon = Date(timeIntervalSince1970: 1_800_000_000)

@Test func thePacerWaitsOutTheRestOfTheFrameGap() {
    #expect(abs(FramePacer().waitBeforeAsking(sinceLastFrame: 0.005) - 0.015) < 0.0001)
}

@Test func thePacerNeverWaitsWhenWeAreAlreadyLate() {
    #expect(FramePacer().waitBeforeAsking(sinceLastFrame: 0.5) == 0)
}

@Test func theMeterNeedsTwoFramesBeforeItHasAnOpinion() {
    var meter = FrameRateMeter()
    meter.frameArrived(at: noon)
    #expect(meter.framesPerSecond == 0)
}

@Test func theMeterReadsSteadyFramesExactly() {
    var meter = FrameRateMeter()
    for frame in 0..<10 { meter.frameArrived(at: noon.addingTimeInterval(Double(frame) * 0.04)) }
    #expect(abs(meter.framesPerSecond - 25) < 0.01)
}

@Test func oneSlowFrameOnlyNudgesTheMeter() {
    var meter = FrameRateMeter()
    for frame in 0..<10 { meter.frameArrived(at: noon.addingTimeInterval(Double(frame) * 0.04)) }
    meter.frameArrived(at: noon.addingTimeInterval(0.36 + 0.5))
    #expect(meter.framesPerSecond > 20)
}

private let atTheDesk = Situation(onACall: false, nobodyLooking: false)
private let onACall = Situation(onACall: true, nobodyLooking: false)
private let away = Situation(onACall: false, nobodyLooking: true)

@Test func neverProbeWhileSomeoneIsAtTheDeskWithoutACall() {
    var policy = ProbePolicy(startingAt: noon)
    let probes = (0..<3600).filter { policy.shouldProbe(atTheDesk, at: noon.addingTimeInterval(Double($0))) }
    #expect(probes.isEmpty)
}

@Test func neverProbeDuringACall() {
    var policy = ProbePolicy(startingAt: noon)
    let probe = policy.shouldProbe(onACall, at: noon.addingTimeInterval(600))
    #expect(probe == false)
}

@Test func probeHalfAMinuteAfterTheCallEnds() {
    var policy = ProbePolicy(startingAt: noon)
    _ = policy.shouldProbe(onACall, at: noon)
    let tooSoon = policy.shouldProbe(atTheDesk, at: noon.addingTimeInterval(10))
    let onTime = policy.shouldProbe(atTheDesk, at: noon.addingTimeInterval(41))
    #expect(tooSoon == false)
    #expect(onTime)
}

@Test func theAfterCallProbeOnlyHappensOnce() {
    var policy = ProbePolicy(startingAt: noon)
    _ = policy.shouldProbe(onACall, at: noon)
    _ = policy.shouldProbe(atTheDesk, at: noon.addingTimeInterval(1))
    policy.probeFoundSomeoneWatching(at: noon.addingTimeInterval(31))
    let again = policy.shouldProbe(atTheDesk, at: noon.addingTimeInterval(60))
    #expect(again == false)
}

@Test func probeRightAwayWhenNobodyIsLooking() {
    var policy = ProbePolicy(startingAt: noon)
    let probe = policy.shouldProbe(away, at: noon)
    #expect(probe)
}

@Test func probesWhileAwayBackOff() {
    var policy = ProbePolicy(startingAt: noon, shortestAwayInterval: 60)
    policy.probeFoundSomeoneWatching(at: noon)
    let early = policy.shouldProbe(away, at: noon.addingTimeInterval(59))
    let due = policy.shouldProbe(away, at: noon.addingTimeInterval(60))
    policy.probeFoundSomeoneWatching(at: noon.addingTimeInterval(60))
    let stillWaiting = policy.shouldProbe(away, at: noon.addingTimeInterval(170))
    #expect(early == false)
    #expect(due)
    #expect(stillWaiting == false)
}

@Test func awayBackoffStopsAtFifteenMinutes() {
    var policy = ProbePolicy(startingAt: noon)
    for _ in 0..<20 { policy.probeFoundSomeoneWatching(at: noon) }
    #expect(policy.awayInterval == 900)
}

@Test func everyFiresTheFirstTimeItIsAsked() {
    var timer = Every(5)
    let due = timer.isDue(at: noon)
    #expect(due)
}

@Test func everyStaysQuietInsideTheInterval() {
    var timer = Every(5)
    _ = timer.isDue(at: noon)
    let due = timer.isDue(at: noon.addingTimeInterval(4))
    #expect(due == false)
}

@Test func everyFiresAgainAfterAReset() {
    var timer = Every(5)
    _ = timer.isDue(at: noon)
    timer.reset()
    let due = timer.isDue(at: noon.addingTimeInterval(1))
    #expect(due)
}

@Test func timingStatsStaySilentUnlessAskedFor() {
    var stats = TimingStats(enabled: false)
    stats.record(.ask, since: noon)
    let summary = stats.summaryIfDue(at: Date().addingTimeInterval(60))
    #expect(summary == nil)
}

@Test func timingStatsReportAveragesWhenEnabled() {
    var stats = TimingStats(enabled: true)
    stats.count(.notReady)
    let summary = stats.summaryIfDue(at: Date().addingTimeInterval(60))
    #expect(summary?.contains("not ready x1") == true)
}

