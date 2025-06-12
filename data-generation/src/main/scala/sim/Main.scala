package sim

import JsonFormats._
import scala.util.{Random, Using, Try}
import java.io.{FileWriter, PrintWriter}
import play.api.libs.json._
import scala.io.Source

object Main extends App {
  val defaultWidth = 20
  val defaultHeight = 20
  val defaultOnFireTreePercent = 15
  val defaultOnFireGrassPercent = 20
  val defaultEnableWind = 0
  val defaultWindAngle = 0
  val defaultWindStrength = 1

  // Parse command-line args or fall back to defaults
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
    s"Using parameters: width=$width, height=$height, onFireTreesPercent=$onFireTreesPercent, onFireGrassPercent=$onFireGrassPercent, isWindEnable=$enableWind, windAngle=$windAngleArg, windStrength=$windStrengthArg"
  )

  var grid = new Grid(width, height)
    .igniteRandomFires(onFireTreesPercent, onFireGrassPercent)

  // Write metadata and the true initial frame
  Using.resource(new PrintWriter("assets/simulation_stream.ndjson")) { out =>
    val metadata = Json.obj("width" -> width, "height" -> height)
    out.println(Json.stringify(metadata))
    out.println(Json.stringify(Json.toJson(grid.encodeCells)))
  }

  Thread.sleep(100)

  // Append subsequent frames in loop
  val out = new FileWriter("assets/simulation_stream.ndjson", true)
  var lastStepSeen: Boolean = false

  while (true) {
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

      grid = grid.nextStep(isWindEnabled, windAngle, windStrength)

      out.write(Json.stringify(Json.toJson(grid.encodeCells)) + "\n")
      out.flush()

      if (doStep) {
        val updated = control.as[JsObject] + ("step" -> JsBoolean(false))
        Using.resource(new PrintWriter("assets/sim_control.json")) { writer =>
          writer.println(Json.prettyPrint(updated))
        }
      }

      Thread.sleep(100)
    } else {
      Thread.sleep(20)
    }
  }
}
