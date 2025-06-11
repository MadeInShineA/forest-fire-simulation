package sim

import scala.util.{Random, Using}
import play.api.libs.json._

// === Cell Types ===
sealed trait CellType extends Serializable
case object Water extends CellType
case object Grass extends CellType
case object Tree extends CellType
case object BurningTree1 extends CellType
case object BurningTree2 extends CellType
case object BurningTree3 extends CellType
case object BurningGrass extends CellType
case class Ash(deadSteps: Int) extends CellType
case class BurnedGrass(deadSteps: Int) extends CellType

case class Cell(cellType: CellType)

// === JSON Serialization for Compact Export ===
object JsonFormats {
  implicit val cellTypeWrites: Writes[CellType] = Writes {
    case Water          => JsString("W")
    case Grass          => JsString("G")
    case Tree           => JsString("T")
    case BurningTree1   => JsString("*")
    case BurningTree2   => JsString("**")
    case BurningTree3   => JsString("***")
    case BurningGrass   => JsString("+")
    case Ash(_)         => JsString("A")
    case BurnedGrass(_) => JsString("-")
  }
}

// === Grid ===
case class Grid(
    width: Int,
    height: Int,
    cells: Vector[Vector[Cell]],
    ashRegrowSteps: Int = 7,
    burnedGrassRegrowSteps: Int = 4,
    treeIgniteProb: Double = 0.2,
    grassIgniteProb: Double = 0.4,
    windStrengthFactor: Double = 20.0,
    windMinBoost: Double = 0.2,
    windStrongMin: Int = 25,
    fireJumpBaseChance: Double = 0.01,
    fireJumpDistFactor: Double = 10.0,
    ashToTreeProb: Double = 0.5,
    burnedGrassToGrassProb: Double = 0.85
) {

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
          Cell(BurningTree1)
        else if (selectedGrass.contains((xCoord, y)))
          Cell(BurningGrass)
        else
          cell
      }
    }

    copy(
      cells = newCells,
      ashRegrowSteps = ashRegrowSteps,
      burnedGrassRegrowSteps = burnedGrassRegrowSteps,
      treeIgniteProb = treeIgniteProb,
      grassIgniteProb = grassIgniteProb,
      fireJumpBaseChance = fireJumpBaseChance,
      windStrengthFactor = windStrengthFactor,
      windMinBoost = windMinBoost,
      windStrongMin = windStrongMin,
      fireJumpDistFactor = fireJumpDistFactor,
      ashToTreeProb = ashToTreeProb,
      burnedGrassToGrassProb = burnedGrassToGrassProb
    )
  }

  def getCell(x: Int, y: Int): Option[Cell] =
    if (x >= 0 && x < width && y >= 0 && y < height) Some(cells(y)(x)) else None

  def isBurningNeighbor(x: Int, y: Int): Boolean = {
    val burningTrees: Set[CellType] =
      Set(BurningTree1, BurningTree2, BurningTree3)
    val directions = List((-1, 0), (1, 0), (0, -1), (0, 1))
    directions.exists { case (dx, dy) =>
      getCell(x + dx, y + dy) match {
        case Some(Cell(cellType))
            if burningTrees.contains(cellType) || cellType == BurningGrass =>
          true
        case _ => false
      }
    }
  }

  /** Helper: is there any water, grass, or tree adjacent to (x, y)? */
  def hasLivingOrWaterNeighbor(x: Int, y: Int): Boolean = {
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
    neighborDirs.exists { case (dx, dy) =>
      getCell(x + dx, y + dy) match {
        case Some(Cell(Water)) => true
        case Some(Cell(Grass)) => true
        case Some(Cell(Tree))  => true
        case _                 => false
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

    val burningTrees: Set[CellType] =
      Set(BurningTree1, BurningTree2, BurningTree3)

    // Improved wind boost: smooth, exponential scaling with angle
    def windBoost(dx: Int, dy: Int): Double = {
      if (!enableWind) 1.0
      else {
        val nrm = math.sqrt(dx * dx + dy * dy)
        if (nrm == 0) 1.0
        else {
          val dir = (dx / nrm, dy / nrm)
          val alignment = windVec._1 * dir._1 + windVec._2 * dir._2 // -1 to 1
          val rawBoost = math.exp(alignment * windStrength / windStrengthFactor)
          rawBoost.max(windMinBoost).min(2.5)
        }
      }
    }

    // Compute new cells (fire spread and regrowth)
    val newCells = Vector.tabulate(height, width) { (y, x) =>
      cells(y)(x).cellType match {
        case Tree =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy) match {
              case Some(Cell(cellType))
                  if burningTrees.contains(
                    cellType
                  ) || cellType == BurningGrass =>
                val boost = windBoost(dx, dy)
                rand.nextDouble() < treeIgniteProb * boost
              case _ => false
            }
          }
          if (ignites) Cell(BurningTree1) else Cell(Tree)

        case Grass =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy) match {
              case Some(Cell(cellType))
                  if burningTrees.contains(
                    cellType
                  ) || cellType == BurningGrass =>
                val boost = windBoost(dx, dy)
                rand.nextDouble() < grassIgniteProb * boost
              case _ => false
            }
          }
          if (ignites) Cell(BurningGrass) else Cell(Grass)

        case BurningTree1 => Cell(BurningTree2)
        case BurningTree2 => Cell(BurningTree3)
        case BurningTree3 => Cell(Ash(0))
        case BurningGrass => Cell(BurnedGrass(0))

        case Ash(deadSteps) =>
          if (
            deadSteps >= ashRegrowSteps - 1 && hasLivingOrWaterNeighbor(x, y)
          ) {
            if (rand.nextDouble() < ashToTreeProb) Cell(Tree) else Cell(Grass)
          } else {
            Cell(Ash(deadSteps + 1))
          }
        case BurnedGrass(deadSteps) =>
          if (
            deadSteps >= burnedGrassRegrowSteps - 1 && hasLivingOrWaterNeighbor(
              x,
              y
            )
          ) {
            if (rand.nextDouble() < burnedGrassToGrassProb) Cell(Grass)
            else Cell(Tree)
          } else {
            Cell(BurnedGrass(deadSteps + 1))
          }
        case other => Cell(other)
      }
    }

    // Fire jump (spotting): random jump direction around wind
    val jumpChance =
      fireJumpBaseChance * (1 + windStrength / windStrengthFactor)
    val maxJumpDist = math.ceil(windStrength / fireJumpDistFactor).toInt max 1

    // Compute fire jump targets with angular spread
    val jumpCells = (for {
      y <- 0 until height
      x <- 0 until width
      cell = cells(y)(x)
      if burningTrees.contains(cell.cellType) || cell.cellType == BurningGrass
      if rand.nextDouble() < jumpChance
    } yield {
      val dist = rand.nextInt(maxJumpDist) + 1 // [1, maxJumpDist]
      val angleDeviation =
        rand.nextGaussian() * (math.Pi / 8) // 22.5 deg stddev
      val jumpAngle = windRad + angleDeviation

      val dx = math.round(math.sin(jumpAngle) * dist).toInt
      val dy = math.round(-math.cos(jumpAngle) * dist).toInt
      val tx = x + dx
      val ty = y + dy
      (tx, ty)
    }).filter { case (tx: Int, ty: Int) =>
      tx >= 0 && tx < width && ty >= 0 && ty < height &&
      (cells(ty)(tx).cellType == Tree || cells(ty)(tx).cellType == Grass)
    }.toSet

    // Apply fire jumps
    val finalCells = newCells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        if (jumpCells.contains((x, y))) {
          cell.cellType match {
            case Tree  => Cell(BurningTree1)
            case Grass => Cell(BurningGrass)
            case _     => cell
          }
        } else cell
      }
    }

    copy(
      cells = finalCells,
      ashRegrowSteps = ashRegrowSteps,
      burnedGrassRegrowSteps = burnedGrassRegrowSteps,
      treeIgniteProb = treeIgniteProb,
      grassIgniteProb = grassIgniteProb,
      fireJumpBaseChance = fireJumpBaseChance,
      windStrengthFactor = windStrengthFactor,
      windMinBoost = windMinBoost,
      windStrongMin = windStrongMin,
      fireJumpDistFactor = fireJumpDistFactor,
      ashToTreeProb = ashToTreeProb,
      burnedGrassToGrassProb = burnedGrassToGrassProb
    )
  }

  def printGrid(): Unit = {
    for (row <- cells) println(row.map(cellSymbol).mkString(" "))
  }

  private def cellSymbol(cell: Cell): String = cell.cellType match {
    case Water          => "~"
    case Grass          => "."
    case Tree           => "T"
    case BurningTree1   => "*"
    case BurningTree2   => "2"
    case BurningTree3   => "3"
    case BurningGrass   => "+"
    case Ash(_)         => "x"
    case BurnedGrass(_) => "-"
  }

  def encodeCells: Vector[Vector[String]] = {
    cells.map(
      _.map(cell =>
        cell.cellType match {
          case Water          => "W"
          case Grass          => "G"
          case Tree           => "T"
          case BurningTree1   => "*"
          case BurningTree2   => "**"
          case BurningTree3   => "***"
          case BurningGrass   => "+"
          case Ash(_)         => "A"
          case BurnedGrass(_) => "-"
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
