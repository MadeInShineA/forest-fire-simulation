import JsonFormats._
import scala.util.{Try, Using, Random}
import java.io.{FileWriter, PrintWriter}
import play.api.libs.json._
import scala.io.Source

object Main extends App {
  val RUN_FAST = false

  val defaults = List(20, 20, 2, 15, 20, 0, 0, 1)
  val parsedArgs = args.dropWhile(_ == "--").map(_.toIntOption).toList
  val finalArgs =
    (parsedArgs ++ defaults.map(Some(_))).take(8).map(_.getOrElse(0))
  val List(
    width,
    height,
    thunderPercentage,
    fireTree,
    fireGrass,
    windEnabled,
    windAngle,
    windStrength
  ) = finalArgs

  // ==== FIXED RNG SEED (set here) ====
  // val rngSeed = 42
  // val rand = new Random(rngSeed)
  val rand = new Random()
  def writeInitialFiles(grid: Grid): Unit =
    Using.resource(new PrintWriter("res/simulation_stream.ndjson")) { out =>
      val metadata = Json.obj("width" -> width, "height" -> height)
      out.println(Json.stringify(metadata))
      out.println(Json.stringify(Json.toJson(grid.encodeCells)))
    }

  // Tuple is: (thunder, windAngle, windStrength, windEnabled)
  def loadControlState(
      defaults: (Int, Int, Int, Boolean)
  ): (Int, Int, Int, Boolean, Boolean, Boolean) = {
    val (defaultThunder, defaultAngle, defaultStrength, defaultEnabled) =
      defaults
    val controlJson = Try(
      Json.parse(Source.fromFile("res/sim_control.json").mkString)
    ).getOrElse(Json.obj())
    (
      (controlJson \ "thunderPercentage").asOpt[Int].getOrElse(defaultThunder),
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
    Using.resource(new PrintWriter("res/sim_control.json")) { writer =>
      writer.println(Json.prettyPrint(controlJson))
    }

  def loop(
      grid: Grid,
      out: FileWriter,
      lastStepSeen: Boolean,
      defaultControl: (Int, Int, Int, Boolean)
  ): Unit = {
    val (thunderPct, windAngle, windStrength, windEnabled, paused, step) =
      loadControlState(defaultControl)
    val doStep = step && !lastStepSeen

    if (!paused || doStep) {
      val nextGrid =
        grid.nextStep(thunderPct, windEnabled, windAngle, windStrength)
      writeFrame(out, nextGrid)

      if (doStep) {
        val controlJson = Try(
          Json.parse(Source.fromFile("res/sim_control.json").mkString)
        ).getOrElse(Json.obj())
        controlJson match {
          case obj: JsObject =>
            updateControlJson(obj + ("step" -> JsBoolean(false)))
          case _ => ()
        }
      }

      if (!RUN_FAST) Thread.sleep(100)
      loop(nextGrid, out, step, defaultControl)
    } else {

      if (!RUN_FAST) Thread.sleep(20)
      loop(grid, out, lastStepSeen, defaultControl)
    }
  }

  val initialGrid =
    Grid(width, height, rand).igniteRandomFires(fireTree, fireGrass)
  writeInitialFiles(initialGrid)

  if (!RUN_FAST) Thread.sleep(100)
  Using.resource(new FileWriter("res/simulation_stream.ndjson", true)) { out =>
    loop(
      initialGrid,
      out,
      lastStepSeen = false,
      (
        thunderPercentage, // Int
        windAngle, // Int
        windStrength, // Int
        windEnabled == 1 // Boolean
      )
    )
  }
}
