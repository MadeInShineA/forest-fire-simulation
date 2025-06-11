package sim

import JsonFormats._
import scala.util.{Random, Try, Using}
import java.io.PrintWriter
import play.api.libs.json._
import scala.io.Source

object Main extends App {
  // --- Default parameters ---
  val defaultWidth = 20
  val defaultHeight = 20
  val defaultOnFireTreePercent = 15
  val defaultOnFireGrassPercent = 20
  val defaultEnableWind = 0
  val defaultWindAngle = 0
  val defaultWindStrength = 1

  // --- Parse command-line args or fall back to defaults ---
  val filteredArgs = args.dropWhile(_ == "--")
  val parsedArgs = filteredArgs.map(_.toIntOption).toList
  val finalArgs = (parsedArgs ++ List(
    Some(defaultWidth),
    Some(defaultHeight),
    Some(defaultOnFireTreePercent),
    Some(defaultOnFireGrassPercent),
    Some(defaultEnableWind),
    Some(defaultWindAngle),
    Some(defaultWindStrength)
  )).take(7).map(_.getOrElse(0))

  val Array(
    width,
    height,
    onFireTreesPercent,
    onFireGrassPercent,
    enableWind,
    windAngleArg,
    windStrengthArg
  ) = finalArgs.toArray

  println(
    s"Using parameters: width=$width, height=$height, onFireTreesPercent=$onFireTreesPercent, " +
      s"onFireGrassPercent=$onFireGrassPercent, isWindEnable=$enableWind, " +
      s"windAngle=$windAngleArg, windStrength=$windStrengthArg"
  )

  // --- Read existing control JSON or default ---
  val initialControl = Try(
    Json.parse(Source.fromFile("assets/sim_control.json").mkString)
  ).getOrElse(Json.obj())

  val initWindAngle =
    (initialControl \ "windAngle").asOpt[Int].getOrElse(windAngleArg)
  val initWindStrength =
    (initialControl \ "windStrength").asOpt[Int].getOrElse(windStrengthArg)
  val initIsWindEnabled =
    (initialControl \ "windEnabled").asOpt[Boolean].getOrElse(enableWind == 1)

  // --- Initialize grid WITHOUT an extra step ---
  var grid = new Grid(width, height)
    .igniteRandomFires(onFireTreesPercent, onFireGrassPercent)

  // --- Open NDJSON file for streaming, auto-flush on each println ---
  val out = new PrintWriter(
    "assets/simulation_stream.ndjson",
    "UTF-8",
    autoFlush = true
  )

  // Write metadata and the true initial frame
  val metadata = Json.obj("width" -> width, "height" -> height)
  out.println(Json.stringify(metadata))
  out.println(Json.stringify(Json.toJson(grid.encodeCells)))

  // --- Append subsequent frames in loop (no sleeps!) ---
  var lastStepSeen = false

  while (true) {
    // Re-read the control file each iteration
    val control = Try(
      Json.parse(Source.fromFile("assets/sim_control.json").mkString)
    ).getOrElse(Json.obj())

    val windAngle = (control \ "windAngle").asOpt[Int].getOrElse(windAngleArg)
    val windStrength =
      (control \ "windStrength").asOpt[Int].getOrElse(windStrengthArg)
    val isWindEnabled =
      (control \ "windEnabled").asOpt[Boolean].getOrElse(enableWind == 1)
    val paused = (control \ "paused").asOpt[Boolean].getOrElse(false)
    val step = (control \ "step").asOpt[Boolean].getOrElse(false)

    val doStep = step && !lastStepSeen
    lastStepSeen = step

    if (!paused || doStep) {
      println(s"[DEBUG] Advancing simulation (paused=$paused, doStep=$doStep)")

      // Advance simulation
      grid = grid.nextStep(isWindEnabled, windAngle, windStrength)

      // Emit one JSON line per frame
      out.println(Json.stringify(Json.toJson(grid.encodeCells)))

      // If we stepped only once, clear the step flag
      if (doStep) {
        val updated = control.as[JsObject] + ("step" -> JsBoolean(false))
        Using.resource(new PrintWriter("assets/sim_control.json", "UTF-8")) {
          writer =>
            writer.println(Json.prettyPrint(updated))
        }
      }
    }
    // otherwise, immediately loop and re-check control
  }
}
