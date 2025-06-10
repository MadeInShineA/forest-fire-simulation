package sim

import scala.util.{Random, Using}
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

  def igniteRandomFires(percentTrees: Double, percentGrass: Double): Grid = {
    require(
      percentTrees >= 0 && percentTrees <= 100,
      "percentTrees must be between 0 and 100"
    )
    require(
      percentGrass >= 0 && percentGrass <= 100,
      "percentGrass must be between 0 and 100"
    )

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

    val countTrees = ((treePositions.length * percentTrees) / 100.0).round.toInt
    val countGrass =
      ((grassPositions.length * percentGrass) / 100.0).round.toInt

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

  def nextStep(enableWind: Boolean, windAngle: Int, windStrength: Int): Grid = {
    val rand = new Random()

    // Wind vector (unit)
    val windRad = math.toRadians(windAngle)
    val windVec = (math.sin(windRad), -math.cos(windRad))

    // All 8 neighbor directions
    val neighborDirs = List(
      (-1, -1),
      (0, -1),
      (1, -1),
      (-1, 0),
      (1, 0),
      (-1, 1),
      (0, 1),
      (1, 1)
    )

    // Wind boost for each neighbor
    def windBoost(dx: Int, dy: Int): Double = {
      if (!enableWind) 1.0
      else {
        val nrm = math.sqrt(dx * dx + dy * dy)
        if (nrm == 0) 1.0
        else {
          val dir = (dx / nrm, dy / nrm)
          val alignment = windVec._1 * dir._1 + windVec._2 * dir._2 // -1 to 1

          // If wind is strong (e.g., > 25 km/h), upwind is impossible
          val minBoost =
            if (windStrength >= 25) 0.0
            else 0.2 // Very low but not zero for less strong winds

          val boost = 1.0 + alignment * (windStrength / 20.0)
          if (alignment < -0.7 && windStrength >= 20)
            0.0 // almost upwind, strong wind
          else boost.max(minBoost).min(2.0)
        }
      }
    }

    // Compute new cells (normal fire spread)
    val newCells = Vector.tabulate(height, width) { (y, x) =>
      cells(y)(x).cellType match {
        case Tree =>
          val baseProb = 0.2
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy) match {
              case Some(Cell(BurningTree | BurningGrass)) =>
                val boost = windBoost(dx, dy)
                rand.nextDouble() < baseProb * boost
              case _ => false
            }
          }
          if (ignites) Cell(BurningTree) else Cell(Tree)

        case Grass =>
          val baseProb = 0.4
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy) match {
              case Some(Cell(BurningTree | BurningGrass)) =>
                val boost = windBoost(dx, dy)
                rand.nextDouble() < baseProb * boost
              case _ => false
            }
          }
          if (ignites) Cell(BurningGrass) else Cell(Grass)

        case BurningTree  => Cell(Ash)
        case BurningGrass => Cell(BurnedGrass)
        case other        => Cell(other)
      }
    }

    // Fire jump (spotting)
    val baseJumpChance = 0.01
    val jumpChance = baseJumpChance * (1 + windStrength / 20.0)
    val maxJumpDist = math.ceil(windStrength / 10.0).toInt max 1

    // Compute fire jump targets
    val jumpCells = (for {
      y <- 0 until height
      x <- 0 until width
      cell = cells(y)(x)
      if cell.cellType == BurningTree || cell.cellType == BurningGrass
      if rand.nextDouble() < jumpChance
    } yield {
      // Fire can "jump" to a random distance up to maxJumpDist
      val dist = rand.nextInt(maxJumpDist) + 1 // [1, maxJumpDist]
      val dx = math.round(windVec._1 * dist).toInt
      val dy = math.round(windVec._2 * dist).toInt
      val tx = x + dx
      val ty = y + dy
      (tx, ty)
    }).filter { case (tx, ty) =>
      tx >= 0 && tx < width && ty >= 0 && ty < height &&
      (cells(ty)(tx).cellType == Tree || cells(ty)(tx).cellType == Grass)
    }.toSet

    // Apply fire jumps
    val finalCells = newCells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        if (jumpCells.contains((x, y))) {
          cell.cellType match {
            case Tree  => Cell(BurningTree)
            case Grass => Cell(BurningGrass)
            case _     => cell
          }
        } else cell
      }
    }

    copy(cells = finalCells)
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
