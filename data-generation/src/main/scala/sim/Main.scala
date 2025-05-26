// src/main/scala/sim/Main.scala

package sim

import JsonFormats._
import scala.util.{Random, Using}
import java.io.PrintWriter
import play.api.libs.json._

object Main extends App {
  val defaultWidth = 20
  val defaultHeight = 20
  val defaultOnFireTreePercent = 15
  val defaultOnFireGrassPercent = 20
  val defaultNumberOfSteps = 20

  val filteredArgs = args.dropWhile(_ == "--")
  val parsedArgs = filteredArgs.map(_.toIntOption).toList
  val finalArgs = (parsedArgs ++ List(
    Some(defaultWidth),
    Some(defaultHeight),
    Some(defaultOnFireTreePercent),
    Some(defaultOnFireGrassPercent),
    Some(defaultNumberOfSteps)
  )).take(5).map(_.getOrElse(0))

  val Array(
    width,
    height,
    onFireTreesPercent,
    onFireGrassPercent,
    numberOfSteps
  ) = finalArgs.toArray

  println(
    s"Using parameters: width=$width, height=$height, onFireTreesPercent=$onFireTreesPercent, onFireGrassPercent=$onFireGrassPercent, numberOfSteps=$numberOfSteps"
  )

  val grid = new Grid(width, height)
    .igniteRandomFires(onFireTreesPercent, onFireGrassPercent)

  val steps = (1 to numberOfSteps).scanLeft(grid) { (g, _) =>
    g.nextStep()
  }

  val json = Json.obj(
    "width" -> grid.width,
    "height" -> grid.height,
    "steps" -> steps.map(_.encodeCells)
  )

  Using.resource(new PrintWriter("assets/simulation.json")) { out =>
    out.write(Json.prettyPrint(json))
  }
}
