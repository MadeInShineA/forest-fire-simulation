import JsonFormats._
import scala.util.{Try, Using, Random}
import java.io.{FileWriter, PrintWriter}
import play.api.libs.json._
import scala.io.Source

object Main extends App {
  val RUN_FAST = false

  // Default simulation params: width, height, thunder %, thunder freq, burning trees, burning grass, wind enabled, wind angle, wind strength
  val defaults = List(20, 20, 0, 1, 5, 10, 0, 0, 1)
  val parsedArgs = args.dropWhile(_ == "--").map(_.toIntOption).toList
  val finalArgs =
    (parsedArgs ++ defaults.map(Some(_))).take(9).map(_.getOrElse(0))
  val List(
    width,
    height,
    thunderPercentage,
    stepsBetweenThunder,
    fireTree,
    fireGrass,
    windEnabled,
    windAngle,
    windStrength
  ) = finalArgs

  val rand = new Random()

  def writeInitialFiles(grid: Grid): Unit = {
    Using.resource(new PrintWriter("res/simulation_stream.ndjson")) { out =>
      val metadata = Json.obj("width" -> width, "height" -> height)
      out.println(Json.stringify(metadata))
      out.println(Json.stringify(Json.toJson(grid.encodeCells)))
    }
  }

  def loadControlState(
      defaults: (Int, Int, Int, Int, Boolean)
  ): (Int, Int, Int, Int, Boolean, Boolean, Boolean) = {
    val (
      defaultThunder,
      defaultStepsBetweenThunder,
      defaultAngle,
      defaultStrength,
      defaultEnabled
    ) = defaults
    val controlJson = Try(
      Json.parse(Source.fromFile("res/sim_control.json").mkString)
    ).getOrElse(Json.obj())
    (
      (controlJson \ "thunderPercentage").asOpt[Int].getOrElse(defaultThunder),
      (controlJson \ "stepsBetweenThunder")
        .asOpt[Int]
        .getOrElse(defaultStepsBetweenThunder),
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

  def updateControlJson(controlJson: JsObject): Unit = {
    Using.resource(new PrintWriter("res/sim_control.json")) { writer =>
      writer.println(Json.prettyPrint(controlJson))
    }
  }

  // ---- Main loop with thunder timer logic ----
  def loop(
      grid: Grid,
      out: FileWriter,
      lastStepSeen: Boolean,
      defaultControl: (Int, Int, Int, Int, Boolean),
      stepNum: Int = 0,
      lastThunderSettings: (Int, Int) =
        (thunderPercentage, stepsBetweenThunder),
      thunderStepCounter: Int = 0
  ): Unit = {
    val (
      thunderPct,
      stepsBetweenThunderNow,
      windAngle,
      windStrength,
      windEnabled,
      paused,
      step
    ) = loadControlState(defaultControl)
    val doStep = step && !lastStepSeen

    val thunderParamsChanged =
      (stepsBetweenThunderNow != lastThunderSettings._2)

    // No thunder on very first step!
    val isFirstStep = stepNum == 0

    // Scheduled thunder: interval reached
    val scheduledThunder =
      stepsBetweenThunderNow > 0 && thunderStepCounter >= stepsBetweenThunderNow

    // If interval changed, compare timer: fire now if timer is already over the new interval
    val thunderOnIntervalChange =
      thunderParamsChanged && thunderStepCounter >= stepsBetweenThunderNow && !isFirstStep

    val doThunder =
      (!isFirstStep && (scheduledThunder || thunderOnIntervalChange))

    // Update thunder timer: reset if thunder fires, else increment
    val nextThunderStepCounter =
      if (doThunder) 0
      else thunderStepCounter + 1

    // When interval changes, and timer is LESS than new interval, just keep counting up until the new value is reached.
    val newLastThunderSettings =
      if (thunderParamsChanged && doThunder)
        (thunderPct, stepsBetweenThunderNow)
      else if (thunderParamsChanged)
        (
          thunderPct,
          stepsBetweenThunderNow
        ) // Even if not firing, keep the updated interval for future comparison
      else
        lastThunderSettings

    if (!paused || doStep) {
      val nextGrid =
        grid.nextStep(
          thunderPct,
          windEnabled,
          windAngle,
          windStrength,
          doThunder
        )
      writeFrame(out, nextGrid)

      // After a single-step advance, immediately unset the "step" flag
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
      loop(
        nextGrid,
        out,
        step,
        defaultControl,
        stepNum + 1,
        newLastThunderSettings,
        nextThunderStepCounter
      )
    } else {
      if (!RUN_FAST) Thread.sleep(20)
      loop(
        grid,
        out,
        lastStepSeen,
        defaultControl,
        stepNum,
        lastThunderSettings,
        thunderStepCounter
      )
    }
  }

  // ---- Entrypoint ----
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
        thunderPercentage,
        stepsBetweenThunder,
        windAngle,
        windStrength,
        windEnabled == 1
      ),
      0,
      (thunderPercentage, stepsBetweenThunder),
      0
    )
  }
}
