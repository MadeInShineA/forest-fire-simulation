import scala.util.Random
import play.api.libs.json._
import JsonFormats._

sealed trait CellType extends Serializable

// ─────────────── Basic Terrain and Vegetation ───────────────

case object Water extends CellType
case object Grass extends CellType
case object Tree extends CellType
case object GrowingTree1 extends CellType // Sapling
case object GrowingTree2 extends CellType // Young tree

// ─────────────── Burning Stages ───────────────

case object BurningTree1 extends CellType
case object BurningTree2 extends CellType
case object BurningTree3 extends CellType
case object BurningGrass extends CellType
case object BurningGrowingTree1 extends CellType
case object BurningGrowingTree2_1 extends CellType
case object BurningGrowingTree2_2 extends CellType

// ─────────────── After Fire ───────────────

case object BurnedTree
    extends CellType // Only used with growSteps for regrowth!
case object BurnedGrass
    extends CellType // Only used with growSteps for regrowth!

// ─────────────── Special Event ───────────────

case object Thunder extends CellType

/** Each cell has a type and a growth/regrowth counter (used only for
  * growing/burned types).
  */
case class Cell(cellType: CellType, growSteps: Int = 0)

// ────────────────────── Serialization Formats ──────────────────────

object JsonFormats {
  implicit val cellTypeWrites: Writes[CellType] = Writes {
    case Water                 => JsString("W")
    case Grass                 => JsString("G")
    case Tree                  => JsString("T")
    case GrowingTree1          => JsString("s")
    case GrowingTree2          => JsString("y")
    case BurningTree1          => JsString("*")
    case BurningTree2          => JsString("**")
    case BurningTree3          => JsString("***")
    case BurningGrass          => JsString("+")
    case BurningGrowingTree1   => JsString("!")
    case BurningGrowingTree2_1 => JsString("&")
    case BurningGrowingTree2_2 => JsString("@")
    case BurnedTree            => JsString("A")
    case BurnedGrass           => JsString("-")
    case Thunder               => JsString("TH")
  }
  implicit val vectorStringWrites: Writes[Vector[String]] = Writes { vs =>
    JsArray(vs.map(JsString(_)))
  }
  implicit val vectorVectorStringWrites: Writes[Vector[Vector[String]]] =
    Writes { vvs => JsArray(vvs.map(v => Json.toJson(v))) }
}

// ────────────────────── Main Simulation Grid ──────────────────────

case class Grid(
    width: Int,
    height: Int,
    cells: Vector[Vector[Cell]],
    rand: Random,

    // --- Regrowth timing (in steps = days) ---
    burnedTreeRegrowSteps: Int =
      300, // Days before regrowth can start on burned tree
    burnedGrassRegrowSteps: Int = 15, // Days before burned grass can recover
    saplingGrowSteps: Int = 60, // Days for sapling → young tree
    youngTreeGrowSteps: Int = 180, // Days for young tree → mature tree

    // --- Ignition probabilities (per burning neighbor, per day) ---
    treeIgniteProb: Double =
      0.02, // Probability a tree ignites per burning neighbor/day
    grassIgniteProb: Double =
      0.08, // Probability grass ignites per burning neighbor/day

    // --- Wind effect parameters ---
    windSteepness: Double = 0.4,
    windMidpoint: Double = 20.0,
    windMaxMult: Double = 7.0,

    // --- Fire jump ("spotting") parameters ---
    fireJumpBaseChance: Double = 0.002,
    fireJumpDistFactor: Double = 3.0,
    fireJumpMaxMult: Double = 5.0,

    // --- Post-fire regeneration probabilities (per day, after regrow delay) ---
    burnedTreeToTreeProb: Double = 0.03,
    burnedGrassToGrassProb: Double = 0.95
) {
  private val neighborDirs = List(
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1)
  )

  def sigmoid(x: Double): Double = 1.0 / (1.0 + math.exp(-x))

  def windAmplifier(
      windStrength: Int,
      steepness: Double,
      midpoint: Double,
      maxMult: Double
  ): Double = {
    val sig = sigmoid(steepness * (windStrength - midpoint))
    1.0 + (maxMult - 1.0) * sig
  }

  def windAdjustedProb(
      baseProb: Double,
      dx: Int,
      dy: Int,
      windVec: (Double, Double),
      windStrength: Int
  ): Double = {
    val nrm = math.sqrt(dx * dx + dy * dy)
    if (nrm == 0) baseProb
    else {
      val dir = (dx / nrm, dy / nrm)
      val alignment = windVec._1 * dir._1 + windVec._2 * dir._2
      val mult = windAmplifier(
        windStrength,
        windSteepness,
        windMidpoint,
        windMaxMult
      )
      (baseProb * (1.0 + alignment) * mult).min(1.0).max(0.0)
    }
  }

  def fireJumped(
      x: Int,
      y: Int,
      windAngle: Int,
      windStrength: Int
  ): Boolean = {
    val windRad = math.toRadians(windAngle)
    val jumpDistances = List(2, 3, 4)
    val jumpMult = windAmplifier(
      windStrength,
      windSteepness,
      windMidpoint + 2.0,
      fireJumpMaxMult
    )
    jumpDistances.exists { dist =>
      val dx = Math.round(Math.sin(windRad) * dist).toInt
      val dy = Math.round(-Math.cos(windRad) * dist).toInt
      getCell(x + dx, y + dy).exists {
        case Cell(burnCt, _) if isBurning(burnCt) =>
          val jumpProb =
            fireJumpBaseChance * jumpMult / (dist * fireJumpDistFactor)
          rand.nextDouble() < jumpProb
        case _ => false
      }
    }
  }

  def igniteRandomFires(percentTrees: Double, percentGrass: Double): Grid = {
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

    val selectedTrees =
      pickN(treePositions, (treePositions.size * percentTrees / 100).toInt)
    val selectedGrass =
      pickN(grassPositions, (grassPositions.size * percentGrass / 100).toInt)

    val newCells = cells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        if (selectedTrees.contains((x, y))) Cell(BurningTree1)
        else if (selectedGrass.contains((x, y))) Cell(BurningGrass)
        else cell
      }
    }
    this.copy(cells = newCells, rand = this.rand)
  }

  def strikeThunder(percentage: Int): Grid = {
    val treePositions = for {
      y <- 0 until height
      x <- 0 until width
      if cells(y)(x).cellType == Tree
    } yield (x, y)

    val numToStrike = math.ceil(treePositions.size * percentage / 100.0).toInt
    val toStrike: Set[(Int, Int)] =
      rand.shuffle(treePositions).take(numToStrike).toSet

    val newCells = cells.zipWithIndex.map { case (row, y) =>
      row.zipWithIndex.map { case (cell, x) =>
        if (toStrike.contains((x, y))) Cell(Thunder)
        else cell
      }
    }
    this.copy(cells = newCells)
  }

  def getCell(x: Int, y: Int): Option[Cell] =
    if (x >= 0 && x < width && y >= 0 && y < height) Some(cells(y)(x)) else None

  def isBurning(cellType: CellType): Boolean = cellType match {
    case BurningTree1 | BurningTree2 | BurningTree3 | BurningGrass |
        BurningGrowingTree1 | BurningGrowingTree2_1 | BurningGrowingTree2_2 =>
      true
    case _ => false
  }

  def isLivingOrWater(cellType: CellType): Boolean = cellType match {
    case Water | Grass | Tree | GrowingTree1 | GrowingTree2 => true
    case _                                                  => false
  }

  def hasLivingOrWaterNeighbor(x: Int, y: Int): Boolean =
    neighborDirs.exists { case (dx, dy) =>
      getCell(x + dx, y + dy).exists(c => isLivingOrWater(c.cellType))
    }

  def encodeCells: Vector[Vector[String]] =
    cells.map(
      _.map(cell => JsonFormats.cellTypeWrites.writes(cell.cellType).as[String])
    )

  /** Main step: fire spread, burning progression, and regrowth. */
  def nextStep(
      enableThunder: Boolean,
      thunderPercentage: Int,
      enableWind: Boolean,
      windAngle: Int,
      windStrength: Int,
      doThunderThisStep: Boolean
  ): Grid = {
    val windRad = math.toRadians(windAngle)
    val windVec = (math.sin(windRad), -math.cos(windRad))
    val newCells = Vector.tabulate(height, width) { (y, x) =>
      val Cell(ct, grow) = cells(y)(x)
      ct match {
        // Mature tree: burns from burning neighbor or jump
        case Tree =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < (
                  if (enableWind)
                    windAdjustedProb(
                      treeIgniteProb,
                      dx,
                      dy,
                      windVec,
                      windStrength
                    )
                  else treeIgniteProb
                )
              case _ => false
            }
          }
          val jump =
            enableWind && !ignites && fireJumped(x, y, windAngle, windStrength)
          if (ignites || jump) Cell(BurningTree1) else Cell(Tree)

        // Sapling: burns, or grows to young tree
        case GrowingTree1 =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < (
                  if (enableWind)
                    windAdjustedProb(
                      treeIgniteProb * 1.2,
                      dx,
                      dy,
                      windVec,
                      windStrength
                    )
                  else treeIgniteProb * 1.2
                )
              case _ => false
            }
          }
          val jump =
            enableWind && !ignites && fireJumped(x, y, windAngle, windStrength)
          if (ignites || jump) Cell(BurningGrowingTree1)
          else if (grow >= saplingGrowSteps) Cell(GrowingTree2)
          else Cell(GrowingTree1, grow + 1)

        // Young tree: burns, or grows to mature tree
        case GrowingTree2 =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < (
                  if (enableWind)
                    windAdjustedProb(
                      treeIgniteProb * 1.1,
                      dx,
                      dy,
                      windVec,
                      windStrength
                    )
                  else treeIgniteProb * 1.1
                )
              case _ => false
            }
          }
          val jump =
            enableWind && !ignites && fireJumped(x, y, windAngle, windStrength)
          if (ignites || jump) Cell(BurningGrowingTree2_1)
          else if (grow >= youngTreeGrowSteps) Cell(Tree)
          else Cell(GrowingTree2, grow + 1)

        // Grass: burns from neighbor, regrows quickly
        case Grass =>
          val ignites = neighborDirs.exists { case (dx, dy) =>
            getCell(x + dx, y + dy).exists {
              case Cell(burnCt, _) if isBurning(burnCt) =>
                rand.nextDouble() < (
                  if (enableWind)
                    windAdjustedProb(
                      grassIgniteProb,
                      dx,
                      dy,
                      windVec,
                      windStrength
                    )
                  else grassIgniteProb
                )
              case _ => false
            }
          }
          val jump =
            enableWind && !ignites && fireJumped(x, y, windAngle, windStrength)
          if (ignites || jump) Cell(BurningGrass) else Cell(Grass)

        // Thunder strikes: instantly turns tree into burning
        case Thunder             => Cell(BurningTree1)
        case BurningTree1        => Cell(BurningTree2)
        case BurningTree2        => Cell(BurningTree3)
        case BurningTree3        => Cell(BurnedTree, 0) // start regrowth timer!
        case BurningGrass        => Cell(BurnedGrass, 0)
        case BurningGrowingTree1 => Cell(BurnedTree, 0)
        case BurningGrowingTree2_1 => Cell(BurningGrowingTree2_2)
        case BurningGrowingTree2_2 => Cell(BurnedTree, 0)

        // Burned tree: count days, regrow as sapling or grass after enough time
        case BurnedTree =>
          if (
            grow >= burnedTreeRegrowSteps - 1 && hasLivingOrWaterNeighbor(x, y)
          ) {
            if (rand.nextDouble() < burnedTreeToTreeProb) Cell(GrowingTree1)
            else Cell(Grass)
          } else Cell(BurnedTree, grow + 1)

        // Burned grass: count days, regrow as grass or sapling after enough time
        case BurnedGrass =>
          if (
            grow >= burnedGrassRegrowSteps - 1 && hasLivingOrWaterNeighbor(
              x,
              y
            )
          ) {
            if (rand.nextDouble() < burnedGrassToGrassProb) Cell(Grass)
            else Cell(GrowingTree1)
          } else Cell(BurnedGrass, grow + 1)

        // Water and unknowns: remain unchanged
        case _ => cells(y)(x)
      }
    }
    val updatedGrid = this.copy(cells = newCells, rand = this.rand)
    if (doThunderThisStep && enableThunder)
      updatedGrid.strikeThunder(thunderPercentage)
    else
      updatedGrid
  }
}

object Grid {
  val InitialTreePercent = 70
  val InitialGrassPercent = 20
  val TotalPercent = 100

  def randomCell(rand: Random): Cell = {
    val roll = rand.nextInt(TotalPercent)
    if (roll < InitialTreePercent)
      Cell(Tree)
    else if (roll < InitialTreePercent + InitialGrassPercent)
      Cell(Grass)
    else
      Cell(Water)
  }

  def apply(width: Int, height: Int, rand: Random): Grid = {
    val cells = Vector.tabulate(height, width)((_, _) => randomCell(rand))
    Grid(width, height, cells, rand)
  }
}
