import JsonFormats._
import scala.util.{Random, Using}
import java.io.PrintWriter
import play.api.libs.json._

object Main extends App {
  import JsonFormats._

  val theGrid = new Grid(20, 10).igniteRandomTrees(5)
  val steps = (0 to 10).scanLeft(theGrid)((grid, _) => grid.nextStep())

  val json = Json.obj(
    "width" -> theGrid.width,
    "height" -> theGrid.height,
    "steps" -> steps.map(_.encodeCells)
  )

  Using.resource(new PrintWriter("viewer/assets/simulation.json")) { out =>
    out.write(Json.prettyPrint(json))
  }

  println("Simulation written to simulation.json")
}
