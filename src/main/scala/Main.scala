import JsonFormats._
import scala.util.{Random, Using}
import java.io.PrintWriter
import play.api.libs.json._

object Main extends App {
  // Default parameters
  val defaultWidth = 20
  val defaultHeight = 20
  val defaultOnFireTreeCount = 5
  val defaultOnFireGrassCount = 5

  // Parse args: width height trees fires
  val Array(width, height, trees, fires) = args.map(_.toInt) ++
    Array(
      defaultWidth,
      defaultHeight,
      defaultOnFireTreeCount,
      defaultOnFireGrassCount
    ).drop(
      args.length
    )

  val grid = new Grid(width, height).igniteRandomFires(trees, fires)
  val steps = (0 to 20).scanLeft(grid)((grid, _) => grid.nextStep())

  val json = Json.obj(
    "width" -> grid.width,
    "height" -> grid.height,
    "steps" -> steps.map(_.encodeCells)
  )

  Using.resource(new PrintWriter("viewer/assets/simulation.json")) { out =>
    out.write(Json.prettyPrint(json))
  }

  println(
    s"Simulation written with size: ${width}x$height, Trees: $trees, Fires: $fires"
  )
}
