import JsonFormats._
import scala.util.{Try, Using, Random}
import java.io.{FileWriter, PrintWriter}
import play.api.libs.json._
import scala.io.Source

object Main extends App {
  val RUN_FAST = true

  // width, height, fireTree, fireGrass, thunderEnabled, thunderPercentage, stepsBetweenThunder , windEnabled, windAngle, windStrength
  val defaults = List(20, 20, 5, 10, 0, 1, 0, 0, 0, 1)

  val parsedArgs = args.dropWhile(_ == "--").map(_.toIntOption).toList
  val finalArgs =
    (parsedArgs ++ defaults.map(Some(_))).take(10).map(_.getOrElse(0))
  val List(
    width,
    height,
    fireTree,
    fireGrass,
    thunderEnabled,
    thunderPercentage,
    stepsBetweenThunder,
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
      defaults: (Int, Int, Boolean, Int, Int, Boolean)
  ): (Int, Int, Boolean, Int, Int, Boolean, Boolean, Boolean) = {
    val (
      defaultThunder,
      defaultStepsBetweenThunder,
      defaultThunderEnabled,
      defaultAngle,
      defaultStrength,
      defaultWindEnabled
    ) = defaults
    val controlJson = Try(
      Json.parse(Source.fromFile("res/sim_control.json").mkString)
    ).getOrElse(Json.obj())
    (
      (controlJson \ "thunderPercentage").asOpt[Int].getOrElse(defaultThunder),
      (controlJson \ "stepsBetweenThunder")
        .asOpt[Int]
        .getOrElse(defaultStepsBetweenThunder),
      (controlJson \ "thunderEnabled")
        .asOpt[Boolean]
        .getOrElse(defaultThunderEnabled),
      (controlJson \ "windAngle").asOpt[Int].getOrElse(defaultAngle),
      (controlJson \ "windStrength").asOpt[Int].getOrElse(defaultStrength),
      (controlJson \ "windEnabled")
        .asOpt[Boolean]
        .getOrElse(defaultWindEnabled),
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

  def loop(
      grid: Grid,
      out: FileWriter,
      lastStepSeen: Boolean,
      defaultControl: (Int, Int, Boolean, Int, Int, Boolean),
      stepNum: Int = 0,
      lastThunderSettings: (Int, Int, Boolean) =
        (thunderPercentage, stepsBetweenThunder, thunderEnabled == 1),
      thunderStepCounter: Int = 0
  ): Unit = {
    val (
      thunderPct,
      stepsBetweenThunderNow,
      thunderEnabledNow,
      windAngle,
      windStrength,
      windEnabled,
      paused,
      step
    ) = loadControlState(defaultControl)
    val doStep = step && !lastStepSeen

    val thunderParamsChanged =
      (stepsBetweenThunderNow != lastThunderSettings._2) || (thunderEnabledNow != lastThunderSettings._3)

    val isFirstStep = stepNum == 0

    val scheduledThunder =
      thunderEnabledNow && stepsBetweenThunderNow > 0 && thunderStepCounter >= stepsBetweenThunderNow

    val thunderOnIntervalChange =
      thunderEnabledNow && thunderParamsChanged && thunderStepCounter >= stepsBetweenThunderNow && !isFirstStep

    val doThunder =
      (!isFirstStep && (scheduledThunder || thunderOnIntervalChange))

    val nextThunderStepCounter =
      if (doThunder) 0
      else thunderStepCounter + 1

    val newLastThunderSettings =
      if (thunderParamsChanged && doThunder)
        (thunderPct, stepsBetweenThunderNow, thunderEnabledNow)
      else if (thunderParamsChanged)
        (thunderPct, stepsBetweenThunderNow, thunderEnabledNow)
      else
        lastThunderSettings

    if (!paused || doStep) {
      val nextGrid =
        grid.nextStep(
          thunderEnabledNow,
          thunderPct,
          windEnabled,
          windAngle,
          windStrength,
          doThunder
        )
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
      if (!RUN_FAST) Thread.sleep(20)
      // The simulation must keep looping while paused, to see when paused changes
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
        thunderEnabled == 1,
        windAngle,
        windStrength,
        windEnabled == 1
      ),
      0,
      (thunderPercentage, stepsBetweenThunder, thunderEnabled == 1),
      0
    )
  }
}
