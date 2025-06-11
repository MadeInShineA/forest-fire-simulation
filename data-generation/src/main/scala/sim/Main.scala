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

  // Arg parsing as before
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

  // (Re)initialize grid
  var grid = new Grid(width, height)
    .igniteRandomFires(onFireTreesPercent, onFireGrassPercent)

  // Overwrite NDJSON on start, write metadata as first line
  Using.resource(new PrintWriter("assets/simulation_stream.ndjson")) { out =>
    val metadata = Json.obj(
      "width" -> width,
      "height" -> height
      // "onFireTreesPercent" -> onFireTreesPercent,
      // "onFireGrassPercent" -> onFireGrassPercent
    )
    out.println(Json.stringify(metadata))
  }

  // Now append frames to NDJSON forever (live mode)
  val out = new FileWriter("assets/simulation_stream.ndjson", true)
  while (true) {
    // Read latest wind params and pause state from control file
    val control = Try(
      Json.parse(Source.fromFile("assets/sim_control.json").mkString)
    ).getOrElse(Json.obj())
    val windAngle = (control \ "windAngle").asOpt[Int].getOrElse(windAngleArg)
    val windStrength =
      (control \ "windStrength").asOpt[Int].getOrElse(windStrengthArg)
    val isWindEnabled =
      (control \ "windEnabled").asOpt[Boolean].getOrElse(enableWind == 1)
    val paused = (control \ "paused").asOpt[Boolean].getOrElse(false)

    if (!paused) {
      // Only advance simulation if not paused
      grid = grid.nextStep(isWindEnabled, windAngle, windStrength)
      println("New simulation step added")
      out.write(Json.stringify(Json.toJson(grid.encodeCells)) + "\n")
      out.flush()
      Thread.sleep(100)

    } else {
      println("Simulation paused")
    }
  }

}
