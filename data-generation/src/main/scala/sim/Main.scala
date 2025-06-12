package sim

import JsonFormats._
import scala.util.{Random, Using, Try}
import java.io.{FileWriter, PrintWriter}
import play.api.libs.json._
import scala.io.Source

object Main extends App {
  val defaults = List(20, 20, 15, 20, 0, 0, 1)
  val parsedArgs = args.dropWhile(_ == "--").map(_.toIntOption).toList
  val finalArgs =
    (parsedArgs ++ defaults.map(Some(_))).take(7).map(_.getOrElse(0))
  val List(
    width,
    height,
    fireTree,
    fireGrass,
    windEnabled,
    windAngle,
    windStrength
  ) = finalArgs

  def writeInitialFiles(grid: Grid): Unit =
    Using.resource(new PrintWriter("assets/simulation_stream.ndjson")) { out =>
      val metadata = Json.obj("width" -> width, "height" -> height)
      out.println(Json.stringify(metadata))
      out.println(Json.stringify(Json.toJson(grid.encodeCells)))
    }

  def loadControlState(
      defaults: (Int, Int, Boolean)
  ): (Int, Int, Boolean, Boolean, Boolean) = {
    val (defaultAngle, defaultStrength, defaultEnabled) = defaults
    val controlJson = Try(
      Json.parse(Source.fromFile("assets/sim_control.json").mkString)
    ).getOrElse(Json.obj())
    (
      (controlJson \ "windAngle").asOpt[Int].getOrElse(defaultAngle),
      (controlJson \ "windStrength").asOpt[Int].getOrElse(defaultStrength),
      (controlJson \ "windEnabled").asOpt[Boolean].getOrElse(defaultEnabled),
      (controlJson \ "paused").asOpt[Boolean].getOrElse(false),
      (controlJson \ "step").asOpt[Boolean].getOrElse(false)
    )
  }

  def writeFrame(out: FileWriter, grid: Grid): Unit = {
    out.write(Json.stringify(Json.toJson(grid.encodeCells)) + "\n")
    out.flush()
  }

  def updateControlJson(controlJson: JsObject): Unit =
    Using.resource(new PrintWriter("assets/sim_control.json")) { writer =>
      writer.println(Json.prettyPrint(controlJson))
    }

  def loop(
      grid: Grid,
      out: FileWriter,
      lastStepSeen: Boolean,
      defaultWind: (Int, Int, Boolean)
  ): Unit = {
    val (windAngle, windStrength, windEnabled, paused, step) = loadControlState(
      defaultWind
    )
    val doStep = step && !lastStepSeen

    if (!paused || doStep) {
      val nextGrid = grid.nextStep(windEnabled, windAngle, windStrength)
      writeFrame(out, nextGrid)

      if (doStep) {
        val controlJson = Try(
          Json.parse(Source.fromFile("assets/sim_control.json").mkString)
        ).getOrElse(Json.obj())
        controlJson match {
          case obj: JsObject =>
            updateControlJson(obj + ("step" -> JsBoolean(false)))
          case _ => ()
        }
      }
      Thread.sleep(100)
      loop(nextGrid, out, step, defaultWind)
    } else {
      Thread.sleep(20)
      loop(grid, out, lastStepSeen, defaultWind)
    }
  }

  val initialGrid = Grid(width, height).igniteRandomFires(fireTree, fireGrass)
  writeInitialFiles(initialGrid)

  Thread.sleep(100)
  Using.resource(new FileWriter("assets/simulation_stream.ndjson", true)) {
    out =>
      loop(
        initialGrid,
        out,
        lastStepSeen = false,
        (windAngle, windStrength, windEnabled == 1)
      )
  }
}
