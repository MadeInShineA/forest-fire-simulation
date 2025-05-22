import scala.util.{Random, Using}
import java.io.PrintWriter
import play.api.libs.json._

// === Cell Types ===
sealed trait CellType
case object Water extends CellType
case object Grass extends CellType
case object Tree extends CellType
case object BurningTree extends CellType
case object BurningGrass extends CellType
case object Ash extends CellType
case object BurnedGrass extends CellType

case class Cell(cellType: CellType)

// === JSON Serialization for Compact Export ===
object JsonFormats {
  implicit val cellTypeWrites: Writes[CellType] = Writes {
    case Water        => JsString("W")
    case Grass        => JsString("G")
    case Tree         => JsString("T")
    case BurningTree  => JsString("*")
    case BurningGrass => JsString("+")
    case Ash          => JsString("A")
    case BurnedGrass  => JsString("-")
  }
}

// === Grid ===
case class Grid(width: Int, height: Int, cells: Vector[Vector[Cell]]) {

  def this(width: Int, height: Int) = {
    this(
      width,
      height,
      Vector.tabulate(height, width)((_, _) => Grid.randomCell())
    )
  }

  def igniteRandomFires(countTrees: Int, countGrass: Int): Grid = {
    val treePositions = for {
      y <- 0 until height
      x <- 0 until width
      if cells(y)(x).cellType == Tree
    } yield (x, y)

    val grassPositions = for {
      y <- 0 until height
      x <- 0 until width
      if cells(y)(x).cellType == Grass
    } yield (x, y)

    val selectedTrees = Random.shuffle(treePositions).take(countTrees)
    val selectedGrass = Random.shuffle(grassPositions).take(countGrass)

    val newCells = cells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, xCoord) =>
        if (selectedTrees.contains((xCoord, y)))
          Cell(BurningTree)
        else if (selectedGrass.contains((xCoord, y)))
          Cell(BurningGrass)
        else
          cell
      }
    }

    copy(cells = newCells)
  }

  def getCell(x: Int, y: Int): Option[Cell] =
    if (x >= 0 && x < width && y >= 0 && y < height) Some(cells(y)(x)) else None

  def isBurningNeighbor(x: Int, y: Int): Boolean = {
    val directions = List((-1, 0), (1, 0), (0, -1), (0, 1))
    directions.exists { case (dx, dy) =>
      getCell(x + dx, y + dy) match {
        case Some(Cell(BurningTree))  => true
        case Some(Cell(BurningGrass)) => true
        case _                        => false
      }
    }
  }

  def nextStep(): Grid = {
    val rand = new Random()
    val newCells = Vector.tabulate(height, width) { (y, x) =>
      val current = cells(y)(x).cellType
      current match {
        case Tree if isBurningNeighbor(x, y) && rand.nextDouble() < 0.5 =>
          Cell(BurningTree)
        case Grass if isBurningNeighbor(x, y) && rand.nextDouble() < 0.8 =>
          Cell(BurningGrass)
        case BurningTree  => Cell(Ash)
        case BurningGrass => Cell(BurnedGrass)
        case other        => Cell(other)
      }
    }
    copy(cells = newCells)
  }

  def printGrid(): Unit = {
    for (row <- cells) println(row.map(cellSymbol).mkString(" "))
  }

  private def cellSymbol(cell: Cell): String = cell.cellType match {
    case Water        => "~"
    case Grass        => "."
    case Tree         => "T"
    case BurningTree  => "*"
    case BurningGrass => "+"
    case Ash          => "x"
    case BurnedGrass  => "-"
  }

  def encodeCells: Vector[Vector[String]] = {
    cells.map(
      _.map(cell =>
        cell.cellType match {
          case Water        => "W"
          case Grass        => "G"
          case Tree         => "T"
          case BurningTree  => "*"
          case BurningGrass => "+"
          case Ash          => "A"
          case BurnedGrass  => "-"
        }
      )
    )
  }
}

object Grid {
  private val rand = new Random()
  def randomCell(): Cell = {
    rand.nextInt(100) match {
      case n if n < 20 => Cell(Water)
      case n if n < 50 => Cell(Grass)
      case _           => Cell(Tree)
    }
  }
}
