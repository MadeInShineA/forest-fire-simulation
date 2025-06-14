package sim

import scala.util.Random
import play.api.libs.json._
import JsonFormats._

// === Cell Types ===
sealed trait CellType extends Serializable
case object Water extends CellType
case object Grass extends CellType
case object Tree extends CellType
case object Sapling extends CellType
case object YoungTree extends CellType
case object BurningTree1 extends CellType
case object BurningTree2 extends CellType
case object BurningTree3 extends CellType
case object BurningSapling extends CellType
case object BurningYoungTree1 extends CellType
case object BurningYoungTree2 extends CellType
case object BurningGrass extends CellType
case object Thunder extends CellType
case class Ash(deadSteps: Int) extends CellType
case class BurnedGrass(deadSteps: Int) extends CellType
case class Cell(cellType: CellType, growSteps: Int = 0)

object JsonFormats {
  implicit val cellTypeWrites: Writes[CellType] = Writes {
    case Water             => JsString("W")
    case Grass             => JsString("G")
    case Tree              => JsString("T")
    case Sapling           => JsString("s")
    case YoungTree         => JsString("y")
    case BurningTree1      => JsString("*")
    case BurningTree2      => JsString("**")
    case BurningTree3      => JsString("***")
    case BurningSapling    => JsString("!")
    case BurningYoungTree1 => JsString("&")
    case BurningYoungTree2 => JsString("@")
    case BurningGrass      => JsString("+")
    case Thunder           => JsString("TH")
    case Ash(_)            => JsString("A")
    case BurnedGrass(_)    => JsString("-")
  }

  implicit val vectorStringWrites: Writes[Vector[String]] = Writes { vs =>
    JsArray(vs.map(JsString(_)))
  }

  implicit val vectorVectorStringWrites: Writes[Vector[Vector[String]]] =
    Writes { vvs => JsArray(vvs.map(v => Json.toJson(v))) }
}

case class Grid(
    width: Int,
    height: Int,
    cells: Vector[Vector[Cell]],
    ashRegrowSteps: Int = 30,
    burnedGrassRegrowSteps: Int = 7,
    saplingGrowSteps: Int = 8,
    youngTreeGrowSteps: Int = 10,
    treeIgniteProb: Double = 0.03,
    grassIgniteProb: Double = 0.12,
    windStrengthFactor: Double = 10.0,
    windMinBoost: Double = 0.5,
    windStrongMin: Int = 15,
    fireJumpBaseChance: Double = 0.005,
    fireJumpDistFactor: Double = 5.0,
    ashToTreeProb: Double = 0.1,
    burnedGrassToGrassProb: Double = 0.75
) {
  def igniteRandomFires(percentTrees: Double, percentGrass: Double): Grid = {
    val rand = new Random()
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

    def pickN[T](xs: Seq[T], n: Int): Set[T] = rand.shuffle(xs).take(n).toSet

    val countTrees = ((treePositions.length * percentTrees) / 100.0).round.toInt
    val countGrass =
      ((grassPositions.length * percentGrass) / 100.0).round.toInt

    val selectedTrees = pickN(treePositions, countTrees)
    val selectedGrass = pickN(grassPositions, countGrass)

    val newCells = cells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        if (selectedTrees.contains((x, y))) Cell(BurningTree1)
        else if (selectedGrass.contains((x, y))) Cell(BurningGrass)
        else cell
      }
    }

    this.copy(cells = newCells)
  }

  def strikeThunder(percentage: Int): Grid = {
    val rand = new Random()
    val newCells = cells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        cell.cellType match {
          case Tree if rand.nextDouble() < percentage / 100.0 => Cell(Thunder)
          case _                                              => cell
        }
      }
    }
    this.copy(cells = newCells)
  }

  def nextStep(
      thunderPercentage: Int,
      enableWind: Boolean,
      windAngle: Int,
      windStrength: Int
  ): Grid = {
    val rand = new Random()

    val windRad = math.toRadians(windAngle)
    val windVec = (math.sin(windRad), -math.cos(windRad))

    def windBoost(dx: Int, dy: Int): Double = {
      if (!enableWind) 1.0
      else {
        val nrm = math.sqrt(dx * dx + dy * dy)
        if (nrm == 0) 1.0
        else {
          val dir = (dx / nrm, dy / nrm)
          val alignment = windVec._1 * dir._1 + windVec._2 * dir._2
          val rawBoost = math.exp(alignment * windStrength / windStrengthFactor)
          rawBoost.max(windMinBoost).min(2.5)
        }
      }
    }

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

    // -------- 1. MAIN UPDATE PHASE (NO THUNDER YET) ----------
    val newCells = Vector.tabulate(height, width) { (y, x) =>
      val Cell(ct, grow) = cells(y)(x)
      ct match {
        case Tree =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < treeIgniteProb * windBoost(dx, dy)
              case _ => false
            }
          }
          if (ignites) Cell(BurningTree1) else Cell(Tree)

        case Sapling =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < treeIgniteProb * windBoost(dx, dy) * 1.2
              case _ => false
            }
          }
          if (ignites) Cell(BurningSapling)
          else if (grow >= saplingGrowSteps) Cell(YoungTree)
          else Cell(Sapling, grow + 1)

        case YoungTree =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < treeIgniteProb * windBoost(dx, dy) * 1.1
              case _ => false
            }
          }
          if (ignites) Cell(BurningYoungTree1)
          else if (grow >= youngTreeGrowSteps) Cell(Tree)
          else Cell(YoungTree, grow + 1)

        case Grass =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < grassIgniteProb * windBoost(dx, dy)
              case _ => false
            }
          }
          if (ignites) Cell(BurningGrass) else Cell(Grass)

        case Thunder           => Cell(BurningTree1)
        case BurningTree1      => Cell(BurningTree2)
        case BurningTree2      => Cell(BurningTree3)
        case BurningTree3      => Cell(Ash(0))
        case BurningGrass      => Cell(BurnedGrass(0))
        case BurningSapling    => Cell(Ash(0))
        case BurningYoungTree1 => Cell(BurningYoungTree2)
        case BurningYoungTree2 => Cell(Ash(0))

        case Ash(deadSteps) =>
          if (
            deadSteps >= ashRegrowSteps - 1 &&
            hasLivingOrWaterNeighbor(x, y)
          ) {
            if (rand.nextDouble() < ashToTreeProb) Cell(Sapling)
            else Cell(Grass)
          } else Cell(Ash(deadSteps + 1))

        case BurnedGrass(deadSteps) =>
          if (
            deadSteps >= burnedGrassRegrowSteps - 1 &&
            hasLivingOrWaterNeighbor(x, y)
          ) {
            if (rand.nextDouble() < burnedGrassToGrassProb) Cell(Grass)
            else Cell(Sapling)
          } else Cell(BurnedGrass(deadSteps + 1))

        case _ => cells(y)(x)
      }
    }

    val updatedGrid = this.copy(cells = newCells)

    updatedGrid.strikeThunder(thunderPercentage)
  }

  def getCell(x: Int, y: Int): Option[Cell] =
    if (x >= 0 && x < width && y >= 0 && y < height) Some(cells(y)(x)) else None

  def isBurning(cellType: CellType): Boolean = cellType match {
    case BurningTree1 | BurningTree2 | BurningTree3 | BurningGrass |
        BurningSapling | BurningYoungTree1 | BurningYoungTree2 =>
      true
    case _ => false
  }

  def isLivingOrWater(cellType: CellType): Boolean = cellType match {
    case Water | Grass | Tree | Sapling | YoungTree => true
    case _                                          => false
  }

  def isTreeOrGrass(cellType: CellType): Boolean = cellType match {
    case Tree | YoungTree | Sapling | Grass => true
    case _                                  => false
  }

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
      getCell(x + dx, y + dy).exists(c => isLivingOrWater(c.cellType))
    }
  }

  def encodeCells: Vector[Vector[String]] = {
    cells.map(_.map(cell => cellTypeWrites.writes(cell.cellType).as[String]))
  }

  def printGrid(): Unit = {
    for (row <- cells)
      println(row.map(cellSymbol).mkString(" "))
  }

  private def cellSymbol(cell: Cell): String = cell.cellType match {
    case Water             => "~"
    case Grass             => "."
    case Tree              => "T"
    case Sapling           => "s"
    case YoungTree         => "y"
    case BurningTree1      => "*"
    case BurningTree2      => "2"
    case BurningTree3      => "3"
    case BurningGrass      => "+"
    case BurningSapling    => "!"
    case BurningYoungTree1 => "&"
    case BurningYoungTree2 => "@"
    case Thunder           => "TH"
    case Ash(_)            => "x"
    case BurnedGrass(_)    => "-"
  }
}

object Grid {
  private val rand = new Random()
  def apply(width: Int, height: Int): Grid = {
    Grid(width, height, Vector.tabulate(height, width)((_, _) => randomCell()))
  }
  def randomCell(): Cell = rand.nextInt(100) match {
    case n if n < 10 => Cell(Water)
    case n if n < 40 => Cell(Grass)
    case _           => Cell(Tree)
  }
}
