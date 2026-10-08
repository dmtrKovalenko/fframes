// Apple Vision helpers for footage prep.
//   vision mask-video <in.mp4> <outdir> person|instance|foreground [seedX seedY]
//       writes outdir/%05d.png 8-bit masks at source resolution
//   vision lift <image> <outdir>
//       writes one RGBA cutout per foreground instance, cropped to its extent
import AVFoundation
import CoreImage
import Foundation
import ImageIO
import UniformTypeIdentifiers
import Vision

let ci = CIContext(options: [.workingColorSpace: NSNull()])

func writePNG(_ image: CIImage, _ path: String, gray: Bool) {
    let space = gray ? CGColorSpaceCreateDeviceGray() : CGColorSpace(name: CGColorSpace.sRGB)!
    let format: CIFormat = gray ? .L8 : .RGBA8
    guard let cg = ci.createCGImage(image, from: image.extent, format: format, colorSpace: space) else { return }
    let url = URL(fileURLWithPath: path) as CFURL
    guard let dest = CGImageDestinationCreateWithURL(url, UTType.png.identifier as CFString, 1, nil) else { return }
    CGImageDestinationAddImage(dest, cg, nil)
    CGImageDestinationFinalize(dest)
}

func centroid(_ buffer: CVPixelBuffer) -> (Double, Double, Double) {
    CVPixelBufferLockBaseAddress(buffer, .readOnly)
    defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
    let w = CVPixelBufferGetWidth(buffer), h = CVPixelBufferGetHeight(buffer)
    let stride = CVPixelBufferGetBytesPerRow(buffer)
    let base = CVPixelBufferGetBaseAddress(buffer)!
    let format = CVPixelBufferGetPixelFormatType(buffer)
    var sx = 0.0, sy = 0.0, sum = 0.0
    for y in Swift.stride(from: 0, to: h, by: 4) {
        for x in Swift.stride(from: 0, to: w, by: 4) {
            var v: Double
            if format == kCVPixelFormatType_OneComponent32Float {
                v = Double(base.advanced(by: y * stride + x * 4).assumingMemoryBound(to: Float32.self).pointee)
            } else {
                v = Double(base.advanced(by: y * stride + x).assumingMemoryBound(to: UInt8.self).pointee) / 255
            }
            sx += v * Double(x); sy += v * Double(y); sum += v
        }
    }
    if sum < 1 { return (-1, -1, 0) }
    return (sx / sum / Double(w), sy / sum / Double(h), sum)
}

func maskVideo(_ input: String, _ outDir: String, _ mode: String, _ seed: (Double, Double)?) throws {
    let asset = AVURLAsset(url: URL(fileURLWithPath: input))
    let track = asset.tracks(withMediaType: .video)[0]
    let reader = try AVAssetReader(asset: asset)
    let output = AVAssetReaderTrackOutput(track: track, outputSettings: [
        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
    ])
    reader.add(output)
    reader.startReading()
    try FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)
    var index = 0
    var last = seed
    while let sample = output.copyNextSampleBuffer(), let pixels = CMSampleBufferGetImageBuffer(sample) {
        let w = CVPixelBufferGetWidth(pixels), h = CVPixelBufferGetHeight(pixels)
        let handler = VNImageRequestHandler(cvPixelBuffer: pixels, options: [:])
        var mask: CIImage? = nil
        if mode == "person" {
            let request = VNGeneratePersonSegmentationRequest()
            request.qualityLevel = .accurate
            request.outputPixelFormat = kCVPixelFormatType_OneComponent8
            try handler.perform([request])
            if let result = request.results?.first {
                let m = CIImage(cvPixelBuffer: result.pixelBuffer)
                mask = m.transformed(by: CGAffineTransform(scaleX: CGFloat(w) / m.extent.width, y: CGFloat(h) / m.extent.height))
            }
        } else {
            let request: VNImageBasedRequest = mode == "instance" ? VNGeneratePersonInstanceMaskRequest() : VNGenerateForegroundInstanceMaskRequest()
            try handler.perform([request])
            if let result = request.results?.first as? VNInstanceMaskObservation {
                var chosen = result.allInstances
                if let target = last, mode == "instance" || seed != nil {
                    var best: (Int, Double, (Double, Double))? = nil
                    for instance in result.allInstances {
                        let buffer = try result.generateScaledMaskForImage(forInstances: IndexSet(integer: instance), from: handler)
                        let (cx, cy, area) = centroid(buffer)
                        if area <= 0 { continue }
                        let d = hypot(cx - target.0, cy - target.1)
                        if best == nil || d < best!.1 { best = (instance, d, (cx, cy)) }
                    }
                    if let best { chosen = IndexSet(integer: best.0); last = best.2 } else { chosen = [] }
                }
                if !chosen.isEmpty {
                    let buffer = try result.generateScaledMaskForImage(forInstances: chosen, from: handler)
                    mask = CIImage(cvPixelBuffer: buffer)
                }
            }
        }
        let path = String(format: "%@/%05d.png", outDir, index)
        let black = CIImage(color: .black).cropped(to: CGRect(x: 0, y: 0, width: w, height: h))
        writePNG((mask ?? black).cropped(to: CGRect(x: 0, y: 0, width: w, height: h)), path, gray: true)
        index += 1
        if index % 25 == 0 { FileHandle.standardError.write("\(index) frames\n".data(using: .utf8)!) }
    }
    print("\(index) masks -> \(outDir)")
}

func lift(_ input: String, _ outDir: String) throws {
    let url = URL(fileURLWithPath: input)
    let handler = VNImageRequestHandler(url: url, options: [:])
    let request = VNGenerateForegroundInstanceMaskRequest()
    try handler.perform([request])
    guard let result = request.results?.first else { print("no instances"); return }
    try FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)
    let name = url.deletingPathExtension().lastPathComponent
    for instance in result.allInstances {
        let buffer = try result.generateMaskedImage(ofInstances: IndexSet(integer: instance), from: handler, croppedToInstancesExtent: true)
        let image = CIImage(cvPixelBuffer: buffer)
        let path = "\(outDir)/\(name)-\(instance).png"
        writePNG(image, path, gray: false)
        print(path, Int(image.extent.width), Int(image.extent.height))
    }
}

let args = CommandLine.arguments
do {
    switch args.count > 1 ? args[1] : "" {
    case "mask-video":
        let seed = args.count > 6 ? (Double(args[5])!, Double(args[6])!) : nil
        try maskVideo(args[2], args[3], args[4], seed)
    case "lift":
        for file in args[3...] { try lift(file, args[2]) }
    default:
        print("usage: vision mask-video <in> <outdir> person|instance|foreground [seedX seedY] | vision lift <outdir> <images...>")
    }
} catch {
    FileHandle.standardError.write("error: \(error)\n".data(using: .utf8)!)
    exit(1)
}
